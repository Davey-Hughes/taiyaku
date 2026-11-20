#!/usr/bin/env bash
# Points this job's cargo at a build directory kept between runs, when the
# runner mounted the volume for one, and prints what it found. It only ever
# adds: with no volume it leaves CARGO_TARGET_DIR unset, and the job builds in
# ./target from nothing. docs/ci.md, "Build volumes, and what a run keeps".
set -euo pipefail

root=/ci/target
home=/usr/local/cargo
keep_days=14

for mount in "${home}/registry" "${home}/git" "${root}"; do
  if mountpoint -q "${mount}"; then echo "volume: ${mount}"; else echo "no volume: ${mount}"; fi
done

# cargo's download lock is a file in its home directory, which every job's
# container has to itself, so two jobs would unpack one crate into the shared
# registry at once. After one fetch under a lock in the volume, no later cargo
# call in this job downloads anything.
if mountpoint -q "${home}/registry"; then
  # A job's first cargo call installs the toolchain; where none has been made,
  # this one is it, so that no install holds the lock.
  cargo --version >/dev/null || true
  flock -w 300 "${home}/registry/.ci-volumes.lock" cargo fetch --locked \
    || echo "cargo fetch did not finish; the job downloads as it builds"
fi

if ! mountpoint -q "${root}"; then
  echo "not using a build volume: none is mounted at ${root}"
  exit 0
fi

# An exact version only: a floating channel would put two compilers' coverage
# builds in one directory, which scripts/coverage-build-guard.sh refuses.
channel="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' rust-toolchain.toml)"
if ! [[ "${channel}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "not using a build volume: rust-toolchain.toml's channel '${channel}' is not an exact version"
  exit 0
fi

# One run per directory for the whole job. cargo's build lock ends with the
# build, while the tests and the coverage report go on reading the directory.
# A lock file is opened to append, here and below, so that one planted as a
# link to another file empties nothing.
dir=
for slot in a b; do
  exec 9>>"${root}/${channel}-${slot}.lock"
  if flock -n 9; then
    dir="${root}/${channel}-${slot}"
    break
  fi
done
if [ -z "${dir}" ]; then
  echo "not using a build volume: both directories for ${channel} are in use by other runs"
  exit 0
fi
mkdir -p "${dir}"
touch "${dir}/.last-used"

# Other toolchains' directories only: the pinned one's second directory is for
# a second run at once, which would otherwise find it gone when it is needed.
# A directory's lock is taken before it is removed, so a run building in it
# keeps it, and it is renamed first, so a removal cut short leaves nothing
# under a name a run builds in; a later run removes what is left. A directory
# with no mark gets a mark, as its age is unknown.
for other in "${root}"/*/; do
  other="${other%/}"
  if [ ! -d "${other}" ] || [ "${other%-[ab]}" = "${root}/${channel}" ]; then continue; fi
  if [[ "${other}" = *.removing ]]; then
    echo "removing ${other}: an earlier removal did not finish"
    rm -rf "${other:?}" || echo "could not remove ${other}"
  elif [ ! -e "${other}/.last-used" ]; then
    touch "${other}/.last-used" || true
  elif [ -n "$(find "${other}/.last-used" -mmin "+$((keep_days * 24 * 60))")" ]; then
    (
      exec 8>>"${other}.lock" || exit 0
      flock -n 8 || exit 0
      echo "removing ${other}: not used for ${keep_days} days"
      mv "${other:?}" "${other:?}.removing" && rm -rf "${other:?}.removing"
    ) || echo "could not remove ${other}"
  fi
done

# The clone was made before the lock was held, and cargo takes outputs newer
# than a source file as built from it. The refresh records the new times in
# the index, or git diff-files would report every file as changed.
find . -type f -exec touch -c {} +
git update-index -q --refresh

du -sh "${root}"/*/ "${home}/registry" "${home}/git" 2>/dev/null || true

# Written before the lock is kept: a write that fails ends the script with the
# lock released, not held all job beside a build in ./target. Incremental
# compilation is off because its state in a kept directory is an earlier
# run's, which this run would build from. ./target is made because the
# coverage report is written into it and no cargo build makes it now.
mkdir -p target
printf 'CARGO_TARGET_DIR=%s\nCARGO_INCREMENTAL=0\n' "${dir}" >> "${GITHUB_ENV}"

# Keeps the lock once this step has ended: the sleeper inherits descriptor 9
# and lives until the job's container is removed.
setsid sleep infinity </dev/null >/dev/null 2>&1 &

echo "build directory: ${dir}"
