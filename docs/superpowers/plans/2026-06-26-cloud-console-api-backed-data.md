# Cloud Console API-Backed Data Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the built-in cloud console read point mappings and release apply state from the Rust cloud API instead of page-local constants.

**Architecture:** Keep edge-facing config contracts in `edge-core`, add read-only projection helpers to `cloud-control`, expose console-oriented read APIs from `cloud-api`, and keep React pages presentational by passing API-loaded data from `App`. The first version seeds an in-memory demo edge config so the built-in console has meaningful data immediately after startup.

**Tech Stack:** Rust 2021, Axum, Serde, Tokio, Tower, React 18, TypeScript, Vite, Vitest.

---

## File Structure

- Modify `crates/cloud-control/src/store.rs` to expose read iterators for edges, config packages, and releases.
- Modify `crates/cloud-api/src/state.rs` to seed a demo edge config package and release into `AppState::default()`.
- Modify `crates/cloud-api/src/api.rs` to add `GET /api/point-mappings` and `GET /api/releases`.
- Modify `crates/cloud-api/tests/api.rs` with failing tests for the new read APIs.
- Modify `web/console/src/api/types.ts` and `web/console/src/api/client.ts` to add typed fetchers.
- Modify `web/console/src/api/client.test.ts` with failing client tests.
- Modify `web/console/src/App.tsx`, `PointMappingsPage.tsx`, and `ReleasesPage.tsx` so pages render API-loaded data with existing static data as fallback.

## Task 1: Cloud Read Models And API Routes

**Files:**
- Modify: `crates/cloud-control/src/store.rs`
- Modify: `crates/cloud-api/src/state.rs`
- Modify: `crates/cloud-api/src/api.rs`
- Test: `crates/cloud-api/tests/api.rs`

- [ ] **Step 1: Write failing API tests**

Add two tests to `crates/cloud-api/tests/api.rs`:

```rust
#[tokio::test]
async fn point_mappings_endpoint_returns_seeded_config_points() {
    let response = app(AppState::default())
        .oneshot(Request::get("/api/point-mappings").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload[0]["pointId"], "pressure");
    assert_eq!(payload[0]["address"], "holding_register:40001");
}

#[tokio::test]
async fn releases_endpoint_returns_seeded_apply_results() {
    let response = app(AppState::default())
        .oneshot(Request::get("/api/releases").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(payload["draftVersion"], "2026.06.26-001");
    assert_eq!(payload["applyResults"][0]["edgeId"], "edge-dev");
}
```

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cargo test -p cloud-api --test api
```

Expected: the two new tests fail with `404 Not Found` because the routes do not exist.

- [ ] **Step 3: Implement read helpers, demo state, and API projections**

Add store read methods:

```rust
pub fn config_packages(&self) -> impl Iterator<Item = &EdgeConfigPackage> {
    self.config_packages.values()
}

pub fn releases(&self) -> impl Iterator<Item = &ReleaseRecord> {
    self.releases.values()
}
```

Replace derived `Default` for `AppState` with a manual default that creates a demo `EdgeConfigPackage` containing a Modbus TCP connection, `pressure` mapping at `holding_register:40001`, and a release record.

Add routes:

```rust
.route("/api/point-mappings", get(point_mappings))
.route("/api/releases", get(releases).post(create_release))
```

Expose response structs with camelCase serde names for the console.

- [ ] **Step 4: Run API tests and commit**

Run:

```bash
cargo test -p cloud-api --test api
cargo test -p cloud-api
```

Expected: all cloud API tests pass.

Commit:

```bash
git add crates/cloud-control/src/store.rs crates/cloud-api/src/state.rs crates/cloud-api/src/api.rs crates/cloud-api/tests/api.rs
git commit -m "feat: expose console config read APIs"
```

## Task 2: Frontend API Client

**Files:**
- Modify: `web/console/src/api/types.ts`
- Modify: `web/console/src/api/client.ts`
- Test: `web/console/src/api/client.test.ts`

- [ ] **Step 1: Write failing client tests**

Add tests for:

```ts
fetchPointMappings(fetchMock)
fetchReleaseList(fetchMock)
```

Expected data must include `pressure`, `holding_register:40001`, `draftVersion`, and `edge-dev`.

- [ ] **Step 2: Run tests and verify red**

Run:

```bash
cd web/console
npm test -- --run src/api/client.test.ts
```

Expected: fails because the new client functions are missing.

- [ ] **Step 3: Implement typed fetchers**

Add `PointMappingResponse`, `ApplyResultResponse`, and `ReleaseListResponse` to `types.ts`. Add `fetchPointMappings` and `fetchReleaseList` to `client.ts`, reusing the same status-checking helper as `fetchSummary`.

- [ ] **Step 4: Run tests and commit**

Run:

```bash
cd web/console
npm test -- --run src/api/client.test.ts
```

Expected: tests pass.

Commit:

```bash
git add web/console/src/api
git commit -m "feat: add console data api client"
```

## Task 3: Frontend Pages Consume API Data

**Files:**
- Modify: `web/console/src/App.tsx`
- Modify: `web/console/src/pages/PointMappingsPage.tsx`
- Modify: `web/console/src/pages/ReleasesPage.tsx`
- Test: `web/console/src/pages/PointMappingsPage.test.tsx`

- [ ] **Step 1: Keep page tests green while adding props**

Update `PointMappingsPage` to accept an optional `points` prop and keep its current fallback rows for isolated tests.

- [ ] **Step 2: Load data in App**

Use one `Promise.all` in `App` to fetch summary, point mappings, and releases together:

```ts
Promise.all([fetchSummary(), fetchPointMappings(), fetchReleaseList()])
```

Store the results in state and pass them into `PointMappingsPage` and `ReleasesPage`.

- [ ] **Step 3: Verify frontend**

Run:

```bash
cd web/console
npm test -- --run
npm run build
```

Expected: all frontend tests and build pass.

Commit:

```bash
git add web/console/src/App.tsx web/console/src/pages/PointMappingsPage.tsx web/console/src/pages/ReleasesPage.tsx web/console/dist
git commit -m "feat: load console pages from api"
```

## Task 4: Final Verification

**Files:**
- Update generated `web/console/dist` after the final Vite build.

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

Open `http://127.0.0.1:8080`, click `点位配置`, and verify the point page shows `holding_register:40001` from the API-backed page data.

- [ ] **Step 3: Commit final dist or verification-only changes**

If `npm run build` changes `web/console/dist`, commit the updated assets:

```bash
git add web/console/dist
git commit -m "chore: refresh console build assets"
```

## Self-Review

- Spec coverage: this plan extends the approved cloud console design by wiring read-only cloud configuration data into the UI.
- Placeholder scan: no placeholders remain; each task names exact files, commands, and expected failures.
- Type consistency: API camelCase fields match TypeScript `PointMappingResponse` and `ReleaseListResponse` naming.
