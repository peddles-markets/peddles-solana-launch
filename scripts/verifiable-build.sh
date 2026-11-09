#!/usr/bin/env bash
# The binary that gets deployed: built the way `solana-verify build` builds it, so an explorer's
# "verified" badge and `solana-verify verify-from-repo` reproduce it byte for byte.
#
#   bash solana/scripts/verifiable-build.sh [workspace dir]     # default: solana/ of this checkout
#
# What `solana-verify build --library-name peddles_launch` does, read from its source on 2026-11-08
# (github.com/Ellipsis-Labs/solana-verifiable-build, src/main.rs `build`, src/image_config.rs):
#   - the image is picked from `[workspace.metadata.cli] solana = "4.1.2"` in Cargo.toml, and pinned
#     by DIGEST: solanafoundation/solana-verifiable-build@sha256:2e0b78f4…473b4f for 4.1.2;
#   - the workspace is mounted at the image's own working directory (`docker run --rm <image> pwd`);
#   - it runs, in the program's directory: cargo build-sbf [--arch X] -- --config
#     'registries.crates-io.protocol="sparse"' --locked --manifest-path <program>/Cargo.toml
#   - the hash it prints is sha256 of target/deploy/peddles_launch.so with trailing zeros stripped
#     (get_binary_hash); this script prints that as verify_hash, next to the plain sha256.
# If `solana-verify` is installed this script calls it; otherwise it runs the same docker commands.
#
# --arch v3: the devnet proof ran an SBPF v3 binary (ELF e_flags 3, read 2026-11-08 on program data
# 6TTYjoXi…APRB). The same flag must be given to `solana-verify verify-from-repo`.
#
# Nothing here holds a key or talks to a cluster. It writes target/ inside the workspace only.
set -euo pipefail
export MSYS_NO_PATHCONV=1

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ws="$(cd "${1:-$here/..}" && pwd)"
IMAGE="solanafoundation/solana-verifiable-build@sha256:2e0b78f44ee76612e9260c7c988570c5e14de6fbd93e0ab07115ec7054473b4f"
ARCH="${SBF_ARCH:-v3}"
LIB=peddles_launch
PROGRAM_DIR=programs/peddles-launch

[ -f "$ws/Cargo.lock" ] || { echo "no Cargo.lock in $ws: solana-verify refuses a workspace without one" >&2; exit 1; }
grep -q 'solana = "4.1.2"' "$ws/Cargo.toml" || { echo "Cargo.toml does not pin [workspace.metadata.cli] solana = \"4.1.2\"" >&2; exit 1; }
if grep -q 'PEDDLESaunchProgramP1aceho1derDoNotDep1oy11' "$ws/$PROGRAM_DIR/src/lib.rs"; then
  echo "WARNING: declare_id! is still the placeholder. This binary can be inspected, never deployed." >&2
fi

if command -v solana-verify >/dev/null 2>&1; then
  ( cd "$ws" && solana-verify build --library-name "$LIB" --arch "$ARCH" )
else
  workdir="$(docker run --rm "$IMAGE" pwd | tr -d '\r')"
  echo "image $IMAGE, workdir $workdir, arch $ARCH" >&2
  # Memory capped so this PC keeps running; solana-verify reads the same two variables.
  mem="${SVB_DOCKER_MEMORY_LIMIT:-4g}"
  cpus="${SVB_DOCKER_CPU_LIMIT:-2}"
  # The tree is COPIED into the container, not bind-mounted: Docker Desktop refuses a mount from a
  # folder outside its File Sharing list (it refused %TEMP% on 2026-11-08), and a copy needs no
  # setting. `bash -c`, never `bash -lc`: a login shell resets PATH from /etc/profile and drops
  # /usr/local/cargo/bin, so cargo is "command not found" in this image (read 2026-11-08).
  cid="$(docker create --memory "$mem" --cpus "$cpus" -w "$workdir/$PROGRAM_DIR" "$IMAGE" \
    bash -c "cargo build-sbf --arch $ARCH -- --config 'registries.crates-io.protocol=\"sparse\"' --locked --manifest-path $workdir/$PROGRAM_DIR/Cargo.toml")"
  trap 'docker rm -f "$cid" >/dev/null 2>&1' EXIT
  src="$ws"; if command -v cygpath >/dev/null 2>&1; then src="$(cygpath -w "$ws")"; fi
  docker cp "$src/." "$cid:$workdir"
  docker start -a "$cid"
  [ "$(docker inspect -f '{{.State.ExitCode}}' "$cid")" = 0 ] || { echo "cargo build-sbf failed" >&2; exit 1; }
  mkdir -p "$ws/target/deploy"
  dst="$ws/target/deploy"; if command -v cygpath >/dev/null 2>&1; then dst="$(cygpath -w "$dst")"; fi
  docker cp "$cid:$workdir/target/deploy/$LIB.so" "$dst/$LIB.so"
fi

so="$ws/target/deploy/$LIB.so"
[ -f "$so" ] || { echo "no $so after the build" >&2; exit 1; }
bytes=$(wc -c < "$so" | tr -d ' ')
hash=$(sha256sum "$so" | cut -d' ' -f1)
# What `solana-verify get-executable-hash` and `get-program-hash` print: sha256 with the trailing
# zero bytes stripped (get_binary_hash in its src/main.rs, read 2026-11-08). The .so ends in zeros,
# so this differs from sha256sum; compare THIS one with the deployed program, never the plain one.
verify_hash=$(perl -0777 -ne 's/\0+\z//; print' "$so" | sha256sum | cut -d' ' -f1)
echo "{\"binary\":\"target/deploy/$LIB.so\",\"bytes\":$bytes,\"sha256\":\"$hash\",\"verify_hash\":\"$verify_hash\",\"arch\":\"$ARCH\",\"image\":\"$IMAGE\"}"
