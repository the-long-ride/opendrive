#!/usr/bin/env bash
set -euo pipefail

# Read the version from [workspace.package], not a dependency manifest.
workspace_version() {
  awk -F '"' '
    /^\[workspace\.package\]/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
  '
}

if [[ "${ARTIFACT_ONLY:-false}" == "true" ]]; then
  ref_sha="$(git rev-parse HEAD)"
  version="$(workspace_version < Cargo.toml)"
  tag="v${version}-manual-${ref_sha:0:7}"
else
  tag="${REQUESTED_TAG:-}"
  if [[ -z "$tag" && "${REF_TYPE:-}" == "tag" ]]; then
    tag="${GITHUB_REF_NAME:-}"
  fi
  if [[ ! "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][a-zA-Z0-9.-]+)?$ ]]; then
    echo "Enter an existing version tag (e.g. v0.1.0), or select a tag in the workflow ref picker: ${tag:-<empty>}" >&2
    exit 1
  fi
  if ! git show-ref --verify --quiet "refs/tags/$tag"; then
    echo "Release tag does not exist: $tag" >&2
    exit 1
  fi
  ref_sha="$(git rev-parse "refs/tags/${tag}^{commit}")"
  if ! git merge-base --is-ancestor "$ref_sha" refs/remotes/origin/main; then
    echo "Release tag $tag must point to a commit on main" >&2
    exit 1
  fi
  version="$(git show "${ref_sha}:Cargo.toml" | workspace_version)"
  if [[ "v$version" != "$tag" ]]; then
    echo "Version mismatch: Cargo.toml=${version:-<empty>} vs tag=$tag" >&2
    exit 1
  fi
fi

if [[ -z "$version" || -z "$ref_sha" ]]; then
  echo 'Unable to determine release version and commit' >&2
  exit 1
fi

{
  printf 'tag=%s\n' "$tag"
  printf 'version=%s\n' "$version"
  printf 'ref_sha=%s\n' "$ref_sha"
} >> "$GITHUB_OUTPUT"
