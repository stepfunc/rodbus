# Releasing

How to cut a release of Rodbus. Same steps for release candidates (`1.5.0-RC1`)
and final releases (`1.5.0`).

A release is driven by **pushing a git tag**. CI does the publishing (crates.io,
Maven Central, NuGet, docs, and a draft GitHub release). Your job is to land a
"Release X.Y.Z" commit on `main`, then tag it.

## 1. Prepare the release commit (on a branch)

```bash
OLD=1.5.0-RC2
NEW=1.5.0

# Bump the version in every crate, FFI binding build file, and the guide.
grep -rln "$OLD" --include="*.toml" --include="*.txt" --include="*.json" \
  --include="*.csproj" --include="*.xml" . | grep -v Cargo.lock | \
  while read f; do sed -i "s/${OLD//./\\.}/$NEW/g" "$f"; done

# Pull in the latest semver-compatible dependency versions. The FFI binaries
# ship compiled against whatever is in Cargo.lock, so bake in the latest patches
# (and keep `cargo audit` meaningful). Review the diff before committing.
cargo update
```

- Update the top `CHANGELOG.md` header to `### X.Y.Z ###` and add entries for
  user-facing changes since the last release (skip CI-only changes).
- Don't forget `guide/sitedata.json` — it holds the version the published guide
  renders. (The grep above catches it; just verify.)
- Confirm nothing is left behind: `grep -rn "$OLD" . | grep -vE "/target/|\.git/"`
  should print nothing.

## 2. Verify, then merge

```bash
cargo check --workspace
cargo audit          # must pass; resolve any advisories before releasing
```

Open a PR titled `Release X.Y.Z` against `main`, get CI green, and merge.

## 3. Tag the release

The tag name **must exactly match** the version in `Cargo.toml` — CI derives the
published version from it. Bare semver, no `v` prefix; RCs as `X.Y.Z-RCn`.

```bash
git checkout main && git pull
git tag -a 1.5.0 -m "Release 1.5.0"
git push origin 1.5.0
```

## 4. Finish up

- Watch the tagged CI run. The publish jobs are idempotent, so a re-push of the
  same tag safely retries after a transient failure.
- CI opens the GitHub release as a **draft** — review the artifacts, paste in the
  changelog section, and publish it.

> This pipeline mirrors `stepfunc/dnp3`; port release-process fixes between the
> two repos.
