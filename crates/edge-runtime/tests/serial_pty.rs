#![cfg(unix)]

use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::FromRawFd;

use edge_core::{ProtocolConnection, SerialConnectionSettings};
use edge_runtime::{append_modbus_rtu_crc, SerialBusFactory, TokioSerialBusFactory};

#[tokio::test]
async fn production_serial_factory_round_trips_modbus_over_a_pty() {
    let (master, _slave_guard, slave_path) = open_raw_pty();
    let mut request = vec![1, 0x03, 0, 0, 0, 1];
    append_modbus_rtu_crc(&mut request);
    let mut response = vec![1, 0x03, 2, 0, 231];
    append_modbus_rtu_crc(&mut response);
    let expected_request = request.clone();

    let device = std::thread::spawn(move || {
        let mut master = master;
        let mut observed = vec![0_u8; expected_request.len()];
        master
            .read_exact(&mut observed)
            .expect("PTY device should receive the complete request");
        assert_eq!(observed, expected_request);
        master
            .write_all(&response)
            .expect("PTY device should write the response");
        master.flush().expect("PTY device response should flush");
        std::thread::sleep(std::time::Duration::from_millis(150));
        observed
    });

    let connection = ProtocolConnection::modbus_rtu_serial(
        "pty-modbus",
        // macOS rejects IOSSIOSPEED on pseudo terminals; baud 0 skips only that
        // hardware-specific ioctl while retaining the production serial I/O path.
        SerialConnectionSettings::new(slave_path, 0),
    );
    let mut factory = TokioSerialBusFactory;
    let mut bus = factory
        .open(&connection)
        .expect("production serial factory should open the PTY slave");
    let observed_response = bus
        .transact(&request)
        .await
        .expect("production serial transport should complete a frame round trip");

    let mut expected_response = vec![1, 0x03, 2, 0, 231];
    append_modbus_rtu_crc(&mut expected_response);
    assert_eq!(observed_response, expected_response);
    device.join().expect("PTY device thread should finish");
}

fn open_raw_pty() -> (File, File, String) {
    let mut master_fd = -1;
    let mut slave_fd = -1;
    let result = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(result, 0, "openpty should create a pseudo terminal");

    let mut settings = unsafe { std::mem::zeroed::<libc::termios>() };
    assert_eq!(unsafe { libc::tcgetattr(slave_fd, &mut settings) }, 0);
    unsafe { libc::cfmakeraw(&mut settings) };
    settings.c_cc[libc::VMIN] = 1;
    settings.c_cc[libc::VTIME] = 0;
    assert_eq!(
        unsafe { libc::tcsetattr(slave_fd, libc::TCSANOW, &settings) },
        0
    );

    let mut path = [0 as libc::c_char; 1024];
    assert_eq!(
        unsafe { libc::ttyname_r(slave_fd, path.as_mut_ptr(), path.len()) },
        0,
        "PTY slave path should be available"
    );
    let path = unsafe { CStr::from_ptr(path.as_ptr()) }
        .to_str()
        .expect("PTY slave path should be UTF-8")
        .to_string();
    let master = unsafe { File::from_raw_fd(master_fd) };
    let slave = unsafe { File::from_raw_fd(slave_fd) };
    (master, slave, path)
}
