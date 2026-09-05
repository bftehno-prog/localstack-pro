# Changelog

## 1.0.4

- SSL hosts now open over HTTPS instead of falling back to HTTP.
- LocalStack Pro trusts its local CA in the current Windows user certificate store without a console window or elevation prompt.
- Existing SSL hosts repair certificate trust automatically before opening in the browser.

## 1.0.3

- Removed light-corner artifacts from the transparent system-tray panel.
- Refreshed the bilingual in-app documentation with the Wet Asphalt lime theme.
- Reduced unnecessary runtime, disk-metric and hidden log polling work.

## 1.0.2

- Fixed high-severity vulnerabilities in the npm dependency graph.
- Added runtime tests for diagnostic secret redaction and text encoding round-trips.
- Hardened file-manager path authorization against junction and symlink escapes.
- Kept progress labels stable while concurrent non-serial operations finish.
- Synchronized release metadata, documentation, installer names, and signing output with version `1.0.2`.
- Added Rust test execution to the Windows CI workflow.
