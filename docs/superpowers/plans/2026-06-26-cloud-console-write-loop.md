# Cloud Console Write Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the cloud console edit a point mapping draft, save it through the Rust API, publish the latest config package, and observe a simulated edge apply result.

**Architecture:** Keep the write path in `cloud-api` for this MVP while reusing `cloud-control::CloudControlStore` and `cloud-control::ReleaseService`. The API updates the latest in-memory edge config package by cloning it to the next version, then `POST /api/releases/publish` creates a release from the latest config and immediately marks it reported to simulate edge runtime apply. React keeps pages mostly presentational: `App` owns API calls and passes save/publish callbacks down to pages.

**Tech Stack:** Rust 2021, Axum, Serde, Tokio, Tower, React 18, TypeScript, Vite, Vitest, Testing Library.

---

## File Structure

- Modify `crates/cloud-control/src/store.rs` with latest-package read helper.
- Modify `crates/cloud-api/src/api.rs` with `PUT /api/point-mappings/{point_id}` and `POST /api/releases/publish`.
- Modify `crates/cloud-api/tests/api.rs` with write-loop integration tests.
- Modify `web/console/src/api/types.ts`, `web/console/src/api/client.ts`, and `web/console/src/api/client.test.ts` with save/publish client functions.
- Modify `web/console/src/App.tsx` to pass write callbacks and refresh state.
- Modify `web/console/src/pages/PointMappingsPage.tsx` to render editable fields and save status.
- Modify `web/console/src/pages/ReleasesPage.tsx` to trigger publish.
- Update `web/console/dist` after build.

## Task 1: Backend Point Save And Publish APIs

**Files:**
- Modify: `crates/cloud-control/src/store.rs`
- Modify: `crates/cloud-api/src/api.rs`
- Test: `crates/cloud-api/tests/api.rs`

- [ ] **Step 1: Write failing backend tests**

Add tests to `crates/cloud-api/tests/api.rs`:

```rust
#[tokio::test]
async fn point_mapping_update_saves_new_draft_version() {
    let router = app(AppState::default());
    let payload = json!({
        "addressKind": "holding_register",
        "addressValue": "40002",
        "intervalMs": 2000,
        "unit": "MPa"
    });

    let response = router
        .clone()
        .oneshot(
            Request::put("/api/point-mappings/pressure")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let response = router
        .oneshot(Request::get("/api/point-mappings").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["address"], "holding_register:40002");
    assert_eq!(payload[0]["interval"], "2000ms");
}

#[tokio::test]
async fn publish_endpoint_releases_latest_draft_and_reports_apply_result() {
    let router = app(AppState::default());
    let update = json!({
        "addressKind": "holding_register",
        "addressValue": "40002",
        "intervalMs": 2000,
        "unit": "MPa"
    });

    let update_response = router
        .clone()
        .oneshot(
            Request::put("/api/point-mappings/pressure")
                .header("content-type", "application/json")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);

    let publish_response = router
        .oneshot(
            Request::post("/api/releases/publish")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(publish_response.status(), StatusCode::OK);

    let body = to_bytes(publish_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["draftVersion"], "2026.06.26-002");
    assert_eq!(payload["applyResults"][0]["result"], "已应用");
}
```

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cargo test -p cloud-api --test api
```

Expected: the new tests fail because PUT and publish routes do not exist.

- [ ] **Step 3: Implement backend write path**

Add store helper:

```rust
pub fn latest_config_package_for_edge(&self, edge_id: &str) -> Option<&EdgeConfigPackage> {
    self.config_packages()
        .filter(|package| package.edge_id == edge_id)
        .max_by(|left, right| left.version.cmp(&right.version))
}
```

Add `SavePointMappingRequest`, route `PUT /api/point-mappings/{point_id}`, clone the latest `edge-dev` package to the next version, update the point address and interval, and return the updated `PointMappingResponse`.

Add `POST /api/releases/publish`, create a release from the latest `edge-dev` package, immediately mark it reported with the desired version, and return `ReleaseListResponse`.

- [ ] **Step 4: Verify and commit**

Run:

```bash
cargo fmt --all -- --check
cargo test -p cloud-api --test api
cargo test -p cloud-api
```

Expected: all pass.

Commit:

```bash
git add crates/cloud-control/src/store.rs crates/cloud-api/src/api.rs crates/cloud-api/tests/api.rs
git commit -m "feat: add console point save and publish APIs"
```

## Task 2: Frontend Save And Publish Client

**Files:**
- Modify: `web/console/src/api/types.ts`
- Modify: `web/console/src/api/client.ts`
- Test: `web/console/src/api/client.test.ts`

- [ ] **Step 1: Write failing client tests**

Add tests for:

```ts
savePointMapping('pressure', { addressKind: 'holding_register', addressValue: '40002', intervalMs: 2000, unit: 'MPa' }, fetchMock)
publishLatestRelease(fetchMock)
```

Assert that `savePointMapping` uses `PUT /api/point-mappings/pressure` and `publishLatestRelease` uses `POST /api/releases/publish`.

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cd web/console
npm test -- --run src/api/client.test.ts
```

Expected: tests fail because the functions do not exist.

- [ ] **Step 3: Implement client functions**

Add `SavePointMappingRequest` to `types.ts`. Add `savePointMapping` and `publishLatestRelease` to `client.ts`, using JSON headers and the existing status checking helper.

- [ ] **Step 4: Verify and commit**

Run:

```bash
cd web/console
npm test -- --run src/api/client.test.ts
```

Expected: tests pass.

Commit:

```bash
git add web/console/src/api
git commit -m "feat: add console write api client"
```

## Task 3: Editable Point Drawer And Publish Action

**Files:**
- Modify: `web/console/src/App.tsx`
- Modify: `web/console/src/pages/PointMappingsPage.tsx`
- Modify: `web/console/src/pages/PointMappingsPage.css`
- Modify: `web/console/src/pages/ReleasesPage.tsx`
- Test: `web/console/src/pages/PointMappingsPage.test.tsx`

- [ ] **Step 1: Write failing page test**

Update `PointMappingsPage.test.tsx` to pass a spy `onSavePoint`, change the address value input from `40001` to `40002`, click `保存草稿`, and assert the callback receives `pointId: pressure`, `addressKind: holding_register`, `addressValue: 40002`, and `intervalMs: 1000`.

- [ ] **Step 2: Run test and verify red**

Run:

```bash
cd web/console
npm test -- --run src/pages/PointMappingsPage.test.tsx
```

Expected: fails because the drawer fields are not editable and no save callback exists.

- [ ] **Step 3: Implement editable drawer**

Use controlled local state in `PointMappingsPage` for address kind, address value, interval, and unit. Replace the static field cards for protocol mapping and collection policy with labeled inputs where needed. Call `onSavePoint(pointId, request)` from the save button and display a small status text.

Update `App.tsx` to pass an `onSavePoint` callback that calls `savePointMapping`, then refreshes point mappings and releases. Update `ReleasesPage` to accept `onPublish` and call `publishLatestRelease` from `App`.

- [ ] **Step 4: Verify and commit**

Run:

```bash
cd web/console
npm test -- --run
npm run build
```

Expected: all frontend tests and build pass.

Commit:

```bash
git add web/console/src/App.tsx web/console/src/pages/PointMappingsPage.tsx web/console/src/pages/PointMappingsPage.css web/console/src/pages/ReleasesPage.tsx web/console/dist
git commit -m "feat: add editable point save and publish actions"
```

## Task 4: Final Verification

**Files:**
- Update `web/console/dist` if final build changes hashes.

- [ ] **Step 1: Run full verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cd web/console
npm test -- --run
npm run build
```

Expected: all commands exit 0.

- [ ] **Step 2: Browser smoke check**

Run:

```bash
cargo run -p cloud-api
```

Open `http://127.0.0.1:8080`, click `点位配置`, edit the address to `40002`, save, and verify the table shows `holding_register:40002`. Click `配置发布`, click `创建发布`, and verify apply result still shows `已应用`.

## Self-Review

- Spec coverage: this covers cloud-side point config authoring, config version bump, release, and simulated edge apply.
- Placeholder scan: no placeholder instructions remain.
- Type consistency: Rust request fields and TypeScript `SavePointMappingRequest` both use `addressKind`, `addressValue`, `intervalMs`, and `unit`.
