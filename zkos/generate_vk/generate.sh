#!/usr/bin/env bash

set -euo pipefail


# ---- defaults & flags ----

WITH_SUBMODULES=true




verify_clean_worktree() {
# Fail if there are unstaged or staged changes
if ! git diff --quiet --no-ext-diff; then
err "Unstaged changes present in $(pwd)"; return 1
fi
if ! git diff --cached --quiet --no-ext-diff; then
err "Staged but uncommitted changes present in $(pwd)"; return 1
fi
}


checkout_tag() {
local tag="$1"
# Ensure tag exists locally; fetch tags first to be safe
git fetch --tags --prune --force
if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
err "Tag '$tag' not found in repo $(pwd)"; return 1
fi
# Checkout in detached HEAD to avoid moving any branch
git -c advice.detachedHead=false checkout --detach "$tag"
}


update_submodules_if_requested() {
if $WITH_SUBMODULES; then
if [[ -f .gitmodules ]]; then
  git submodule sync --recursive
  git submodule update --init --recursive
fi
fi
}


clone_repo() {
    local url="$1"
    local path="$2"

    if [[ -z "$url" || -z "$path" ]]; then
        printf "ERROR: clone_repo requires url and path\n"
        return 1
    fi

    # Already cloned
    if [[ -d "$path/.git" ]]; then
        printf "Git repository already present at: $path\n"
        return 0
    fi

    # If path exists but is not a git repo, move it aside to avoid partial state
    if [[ -e "$path" ]]; then
        printf "Path exists and is not a git repo.\n"
        return 1
    fi

    mkdir -p "$(dirname "$path")"

    git clone --recurse-submodules "$url" "$path"

    return 0
}

# We can download a non-compat key, as we'll be generating VK on CPU.
download_crs() {
    local crs_url="https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^24.key"

    local crs_path="repos/setup.key"

    if [[ -f "$crs_path" ]]; then
        printf "CRS file already present at: $crs_path\n"
        return 0
    fi

    printf "Downloading CRS file from %s to %s\n" "$crs_url" "$crs_path"

    curl -L -o "$crs_path" "$crs_url"

    if [[ ! -f "$crs_path" ]]; then
        printf "ERROR: Failed to download CRS file\n"
        return 1
    fi

    return 0

}

clone_and_tag() {
    local url="$1"
    local path="$2"
    local tag="$3"

    if [[ -z "$url" || -z "$path" || -z "$tag" ]]; then
        err "clone_and_tag requires url, path and tag"
        return 1
    fi

    clone_repo "$url" "$path" || return 1

    pushd "$path" >/dev/null

    verify_clean_worktree || { popd >/dev/null; return 1; }

    checkout_tag "$tag" || { popd >/dev/null; return 1; }

    update_submodules_if_requested || { popd >/dev/null; return 1; }

    popd >/dev/null

    return 0
}

download_zksync_os_binary() {
  local asset_name="$1"
  local tag="${2:-latest}"
  local target_path="${3:-.}"

  if [[ -z "$asset_name" ]]; then
    printf "Usage: download_zksync_os_binary <asset_name> [tag] [target_path]"
    return 1
  fi

  local asset_url="https://github.com/matter-labs/zksync-os/releases/download/${tag}/${asset_name}"

  mkdir -p "$target_path" || {
    printf "❌ Failed to create directory: $target_path"
    return 1
  }

  local output_file="${target_path%/}/$asset_name"

  http_status=$(curl -sL -w "%{http_code}" -o "$output_file" "$asset_url")

  if [[ "$http_status" != "200" ]]; then
    printf "❌ Download of ${output_file} failed with HTTP status: $http_status\n"
    rm -f "$output_file"
    return 1
  fi

  printf "✅ Downloaded binary: $output_file\n"
}


create_snark_vk() {
    # remove snark_vk_expected.json if exists
    if [[ -f "repos/snark_vk_expected.json" ]]; then
        rm "repos/snark_vk_expected.json"
    fi
    pushd "repos/zkos-wrapper" >/dev/null
    cargo run --bin wrapper --release -- generate-snark-vk --input-binary ../../$env_dir/binaries/multiblock_batch.bin --trusted-setup-file ../setup.key --output-dir ../
    popd >/dev/null

    # check that snark_vk_expected.json is present
    if [[ ! -f "repos/snark_vk_expected.json" ]]; then
        printf "ERROR: Expected file not found: repos/snark_vk_expected.json\n"
        exit 1
    fi
}

create_solidity_files() {
    pushd "repos/era-contracts/tools" >/dev/null
    # Right now, we only update plonk!
    cargo run --bin zksync_verifier_contract_generator --release -- --plonk_input_path ../../snark_vk_expected.json --fflonk_input_path data/fflonk_scheduler_key.json --plonk_output_path ../../L1VerifierPlonk.sol --fflonk_output_path ../l1-contracts/contracts/state-transition/verifiers/L1VerifierFflonk.sol
    popd >/dev/null
}

# ---- main loop ----

if [ $# -lt 1 ]; then
  echo "Usage: $0 <environment dir>" >&2
  exit 1
fi
env_dir="$1"

printf "Running from $env_dir\n"

if [ -f "$env_dir/settings.env" ]; then
  set -a        # export all variables that follow
  . "$env_dir/settings.env"
  set +a
else
  printf "ERROR: Settings file missing\n"
  exit 1
fi



if [[ -z "$ZKOS_WRAPPER_TAG" || -z "$ZKSYNC_OS_TAG" || -z "$ERA_CONTRACTS_TAG" ]]; then
  printf "ERROR: One of the required settings is empty\n"
  exit 1
fi


printf "*** Fetching repositories ***\n"

clone_and_tag git@github.com:matter-labs/zkos-wrapper.git "repos/zkos-wrapper" $ZKOS_WRAPPER_TAG
clone_and_tag git@github.com:matter-labs/zksync-os.git "repos/zksync-os" $ZKSYNC_OS_TAG
clone_and_tag git@github.com:matter-labs/era-contracts.git "repos/era-contracts" $ERA_CONTRACTS_TAG

## TODO: here add some sanity checks.


printf "*** Downloading CRS ***\n"

download_crs

printf "*** Downloading ZKsync OS binary ***\n"
download_zksync_os_binary "multiblock_batch.bin" "$ZKSYNC_OS_TAG" "$env_dir/binaries"

printf "*** Generating snark key ***\n"
create_snark_vk

printf "*** Creating solidity files ***\n"
create_solidity_files

printf "*** Copying values to $env_dir ***\n"
cp repos/snark_vk_expected.json "$env_dir/"
cp repos/L1VerifierPlonk.sol "$env_dir/"


# if UPDATE_ERA_CONTRACTS is set - run the update
if [ "${UPDATE_ERA_CONTRACTS:-}" = "true" ]; then
  pushd "repos/era-contracts/tools" >/dev/null
  cp ../../snark_vk_expected.json data/plonk_scheduler_key.json
  cargo run --bin zksync_verifier_contract_generator --release -- --plonk_input_path data/plonk_scheduler_key.json --fflonk_input_path data/fflonk_scheduler_key.json --plonk_output_path ../l1-contracts/contracts/state-transition/verifiers/L1VerifierPlonk.sol --fflonk_output_path ../l1-contracts/contracts/state-transition/verifiers/L1VerifierFflonk.sol

  # if there is any git diff - create a new branch and push.
  if ! git diff --quiet; then
    git checkout -b "update-vk-from-script-$(date +%Y%m%d%H%M%S)"
    git commit -a -m "Update era contracts - zkos: $ZKSYNC_OS_TAG, wrapper: $ZKOS_WRAPPER_TAG"
    git push origin HEAD
  fi

  popd >/dev/null
fi


exit 0