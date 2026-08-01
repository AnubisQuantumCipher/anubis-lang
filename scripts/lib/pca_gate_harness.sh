#!/usr/bin/env bash
# Strict PCA evidence-generation admission. Intended to be sourced.

pca_gate_setup_error() {
  printf 'PCA_GATE_SETUP_ERROR: %s\n' "$*" >&2
  return 1
}

pca_generate_evidence_bundle() {
  if [[ $# -ne 3 ]]; then
    pca_gate_setup_error "expected <anubis-bin> <program> <output-root>"
    return 2
  fi
  local bin="$1" program="$2" output="$3" rc candidate required
  local -a candidates=()

  if [[ ! -f "$bin" || -L "$bin" || ! -x "$bin" ]]; then
    pca_gate_setup_error "binary must be a regular non-symlink executable: $bin"
    return 2
  fi
  if [[ ! -f "$program" || -L "$program" ]]; then
    pca_gate_setup_error "program must be a regular non-symlink file: $program"
    return 2
  fi
  if [[ -e "$output" || -L "$output" ]]; then
    pca_gate_setup_error "evidence output must not pre-exist: $output"
    return 2
  fi
  mkdir -p "$(dirname "$output")" || {
    pca_gate_setup_error "cannot create output parent: $(dirname "$output")"
    return 2
  }

  "$bin" check "$program" --evidence --out "$output" >/dev/null 2>&1
  rc=$?
  if [[ $rc -ne 0 ]]; then
    pca_gate_setup_error "evidence command exited $rc"
    return 1
  fi
  if [[ ! -d "$output" || -L "$output" ]]; then
    pca_gate_setup_error "evidence command did not create a real output directory: $output"
    return 1
  fi

  shopt -s nullglob
  candidates=("$output"/evidence-*)
  shopt -u nullglob
  if [[ ${#candidates[@]} -ne 1 ]]; then
    pca_gate_setup_error "expected exactly one evidence-* entry, observed ${#candidates[@]}"
    return 1
  fi
  candidate="${candidates[0]}"
  if [[ ! -d "$candidate" || -L "$candidate" ]]; then
    pca_gate_setup_error "evidence entry must be a real directory: $candidate"
    return 1
  fi
  for required in pca.json source.anubis evidence.json MANIFEST.sha256; do
    if [[ ! -f "$candidate/$required" || -L "$candidate/$required" ]]; then
      pca_gate_setup_error "required bundle member must be a regular non-symlink file: $candidate/$required"
      return 1
    fi
  done
  printf '%s\n' "$candidate"
}
