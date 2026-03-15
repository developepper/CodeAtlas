#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  bash scripts/blog/collect_blog_metrics.sh \
    --repos <repos.tsv|repos.csv> \
    --queries <queries.tsv|queries.csv> \
    [--out-dir <dir>] \
    [--codeatlas-bin <bin>]

  bash scripts/blog/collect_blog_metrics.sh compare-prompts <baseline> <with-codeatlas> [--out-file <file>] [--append]

Repo manifest columns:
  repo_id,repo_path,notes

Query manifest columns:
  repo_id,query_type,query_value,notes

Supported query_type values:
  search-symbols
  get-symbol
  file-outline
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

timestamp_utc() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

now_epoch_seconds() {
  if command -v perl >/dev/null 2>&1; then
    perl -MTime::HiRes=time -e 'printf "%.6f\n", time'
    return
  fi

  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import time; print(f"{time.time():.6f}")'
    return
  fi

  date +%s
}

estimate_tokens_from_file() {
  local file="$1"
  local bytes
  bytes=$(wc -c <"$file" | tr -d ' ')
  echo $(((bytes + 3) / 4))
}

csv_escape() {
  local s="$1"
  s=${s//\"/\"\"}
  printf '"%s"' "$s"
}

manifest_delimiter() {
  local manifest="$1"
  case "$manifest" in
    *.tsv)
      printf '\t'
      ;;
    *)
      printf ','
      ;;
  esac
}

extract_value_after_colon() {
  local key="$1"
  local file="$2"
  local line
  line=$(grep -F "$key" "$file" | head -n 1 || true)
  if [[ -z "$line" ]]; then
    printf ""
    return
  fi
  printf "%s" "${line#*:}" | sed 's/^ *//'
}

run_quality_report() {
  local codeatlas_bin="$1"
  local repo_path="$2"
  local report_file="$3"

  "$codeatlas_bin" quality-report "$repo_path" >"$report_file"
}

repo_file_list_cmd() {
  local repo_path="$1"

  if git -C "$repo_path" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git -C "$repo_path" ls-files -z --cached --others --exclude-standard
    return
  fi

  find "$repo_path" \
    -path "$repo_path/.git" -prune -o \
    -type f -print0 | while IFS= read -r -d '' path; do
      printf '%s\0' "${path#"$repo_path"/}"
    done
}

collect_repo_inventory() {
  local repo_path="$1"
  local list_file="$2"
  local file_count_file="$3"
  local line_count_file="$4"

  repo_file_list_cmd "$repo_path" >"$list_file"
  tr -cd '\0' <"$list_file" | wc -c | tr -d ' ' >"$file_count_file"

  if [[ ! -s "$list_file" ]]; then
    echo "0" >"$line_count_file"
    return
  fi

  xargs -0 wc -l <"$list_file" 2>/dev/null | awk 'NF > 1 { sum += $1 } END { print sum + 0 }' >"$line_count_file"
}

collect_repo_metrics() {
  local codeatlas_bin="$1"
  local repos_csv="$2"
  local out_dir="$3"
  local repo_metrics_csv="$out_dir/repo_metrics.csv"
  local summary_file="$out_dir/summary.txt"

  printf "timestamp,repo_id,repo_path,git_sha,file_count,line_count,files_discovered,files_parsed,files_errored,symbols_extracted,total_symbols,semantic_symbols,syntax_symbols,semantic_coverage_percent,avg_confidence,files_with_semantic,total_files,win_rate,wins,losses,ties,kpi_result,notes\n" >"$repo_metrics_csv"

  local delimiter
  delimiter=$(manifest_delimiter "$repos_csv")

  while IFS="$delimiter" read -r repo_id repo_path notes; do
    [[ "$repo_id" == "repo_id" ]] && continue
    [[ -z "$repo_id" ]] && continue

    if [[ ! -d "$repo_path" ]]; then
      echo "skipping missing repo path: $repo_path" >&2
      continue
    fi

    local repo_tmp
    local list_file
    local file_count_file
    local line_count_file
    repo_tmp=$(mktemp)
    list_file=$(mktemp)
    file_count_file=$(mktemp)
    line_count_file=$(mktemp)
    run_quality_report "$codeatlas_bin" "$repo_path" "$repo_tmp"

    local git_sha=""
    if git -C "$repo_path" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      git_sha=$(git -C "$repo_path" rev-parse HEAD 2>/dev/null || true)
    fi

    collect_repo_inventory "$repo_path" "$list_file" "$file_count_file" "$line_count_file"

    local file_count
    local line_count
    file_count=$(cat "$file_count_file")
    line_count=$(cat "$line_count_file")
    line_count=${line_count:-0}

    local files_line
    local symbols_line
    files_line=$(grep -F "Files:" "$repo_tmp" | head -n 1 || true)
    symbols_line=$(grep -F "Symbols:" "$repo_tmp" | head -n 1 || true)

    local files_discovered
    local files_parsed
    local files_errored
    local symbols_extracted
    files_discovered=$(printf "%s\n" "$files_line" | sed -E 's/.*Files:[[:space:]]*([0-9]+) discovered.*/\1/' )
    files_parsed=$(printf "%s\n" "$files_line" | sed -E 's/.*discovered, ([0-9]+) parsed.*/\1/' )
    files_errored=$(printf "%s\n" "$files_line" | sed -E 's/.*parsed, ([0-9]+) errored.*/\1/' )
    symbols_extracted=$(printf "%s\n" "$symbols_line" | sed -E 's/.*Symbols:[[:space:]]*([0-9]+).*/\1/' )

    local total_symbols
    local semantic_symbols
    local syntax_symbols
    local coverage
    local avg_confidence
    local files_with_semantic
    local win_rate
    local wins
    local losses
    local ties
    local kpi_result
    local total_files
    total_symbols=$(extract_value_after_colon "Total symbols:" "$repo_tmp")
    semantic_symbols=$(extract_value_after_colon "Semantic symbols:" "$repo_tmp")
    syntax_symbols=$(extract_value_after_colon "Syntax symbols:" "$repo_tmp")
    coverage=$(extract_value_after_colon "Coverage:" "$repo_tmp" | tr -d '%')
    avg_confidence=$(extract_value_after_colon "Avg confidence:" "$repo_tmp")
    files_with_semantic=$(extract_value_after_colon "Files with semantic:" "$repo_tmp")
    total_files="${files_with_semantic#*/}"
    files_with_semantic="${files_with_semantic%%/*}"
    win_rate=$(extract_value_after_colon "Win rate:" "$repo_tmp" | tr -d '%')
    wins=$(extract_value_after_colon "Wins:" "$repo_tmp")
    losses=$(extract_value_after_colon "Losses:" "$repo_tmp")
    ties=$(extract_value_after_colon "Ties:" "$repo_tmp")
    kpi_result=$(grep -F "Result:" "$repo_tmp" | head -n 1 | awk '{print $2}' || true)

    printf "%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n" \
      "$(timestamp_utc)" \
      "$(csv_escape "$repo_id")" \
      "$(csv_escape "$repo_path")" \
      "$(csv_escape "$git_sha")" \
      "$file_count" \
      "$line_count" \
      "${files_discovered:-0}" \
      "${files_parsed:-0}" \
      "${files_errored:-0}" \
      "${symbols_extracted:-0}" \
      "${total_symbols:-0}" \
      "${semantic_symbols:-0}" \
      "${syntax_symbols:-0}" \
      "${coverage:-0}" \
      "${avg_confidence:-0}" \
      "${files_with_semantic:-0}" \
      "${total_files:-0}" \
      "${win_rate:-0}" \
      "${wins:-0}" \
      "${losses:-0}" \
      "${ties:-0}" \
      "$(csv_escape "$kpi_result")" \
      "$(csv_escape "$notes")" >>"$repo_metrics_csv"

    {
      echo "[$repo_id]"
      cat "$repo_tmp"
      echo
    } >>"$summary_file"

    rm -f "$repo_tmp" "$list_file" "$file_count_file" "$line_count_file"
  done <"$repos_csv"
}

run_timed_query() {
  local codeatlas_bin="$1"
  local repo_id="$2"
  local query_type="$3"
  local query_value="$4"
  local start_s
  local end_s
  local elapsed_s

  start_s=$(now_epoch_seconds)
  case "$query_type" in
    search-symbols)
      "$codeatlas_bin" search-symbols "$query_value" --repo "$repo_id" >/dev/null
      ;;
    get-symbol)
      "$codeatlas_bin" get-symbol "$query_value" >/dev/null
      ;;
    file-outline)
      "$codeatlas_bin" file-outline "$query_value" --repo "$repo_id" >/dev/null
      ;;
    *)
      return 64
      ;;
  esac
  end_s=$(now_epoch_seconds)
  elapsed_s=$(awk -v start="$start_s" -v end="$end_s" 'BEGIN { printf "%.6f", end - start }')
  printf "%s" "$elapsed_s"
}

collect_query_metrics() {
  local codeatlas_bin="$1"
  local queries_csv="$2"
  local out_dir="$3"
  local query_metrics_csv="$out_dir/query_metrics.csv"

  printf "timestamp,repo_id,query_type,query_value,status,elapsed_seconds,notes\n" >"$query_metrics_csv"

  local delimiter
  delimiter=$(manifest_delimiter "$queries_csv")

  while IFS="$delimiter" read -r repo_id query_type query_value notes; do
    [[ "$repo_id" == "repo_id" ]] && continue
    [[ -z "$repo_id" ]] && continue

    local elapsed status
    elapsed=""
    status="ok"
    if ! elapsed=$(run_timed_query "$codeatlas_bin" "$repo_id" "$query_type" "$query_value" 2>/dev/null); then
      status="failed"
      elapsed=""
    fi

    printf "%s,%s,%s,%s,%s,%s,%s\n" \
      "$(timestamp_utc)" \
      "$(csv_escape "$repo_id")" \
      "$(csv_escape "$query_type")" \
      "$(csv_escape "$query_value")" \
      "$(csv_escape "$status")" \
      "$(csv_escape "$elapsed")" \
      "$(csv_escape "$notes")" >>"$query_metrics_csv"
  done <"$queries_csv"
}

compare_prompts() {
  local baseline="$1"
  local with_codeatlas="$2"

  [[ -f "$baseline" ]] || die "baseline prompt not found: $baseline"
  [[ -f "$with_codeatlas" ]] || die "with-CodeAtlas prompt not found: $with_codeatlas"

  printf "label,file,bytes,lines,estimated_tokens\n"
  for pair in "baseline:$baseline" "with_codeatlas:$with_codeatlas"; do
    local label="${pair%%:*}"
    local file="${pair#*:}"
    local bytes lines est
    bytes=$(wc -c <"$file" | tr -d ' ')
    lines=$(wc -l <"$file" | tr -d ' ')
    est=$(estimate_tokens_from_file "$file")
    printf "%s,%s,%s,%s,%s\n" \
      "$(csv_escape "$label")" \
      "$(csv_escape "$file")" \
      "$bytes" \
      "$lines" \
      "$est"
  done
}

write_prompt_metrics() {
  local baseline="$1"
  local with_codeatlas="$2"
  local out_file="$3"
  local append_mode="${4:-0}"

  mkdir -p "$(dirname "$out_file")"

  if [[ "$append_mode" == "1" && -f "$out_file" && -s "$out_file" ]]; then
    compare_prompts "$baseline" "$with_codeatlas" | tail -n +2 >>"$out_file"
    return
  fi

  compare_prompts "$baseline" "$with_codeatlas" >"$out_file"
}

main() {
  require_cmd date
  require_cmd awk
  require_cmd grep
  require_cmd find
  require_cmd wc
  require_cmd xargs

  if [[ $# -gt 0 && "$1" == "compare-prompts" ]]; then
    [[ $# -eq 3 || $# -eq 5 || $# -eq 6 ]] || die "compare-prompts requires <baseline> <with-codeatlas> [--out-file <file>] [--append]"
    local prompt_out=""
    local append_mode="0"
    if [[ $# -ge 5 ]]; then
      [[ "$4" == "--out-file" ]] || die "unknown argument: $4"
      prompt_out="$5"
    fi
    if [[ $# -eq 6 ]]; then
      [[ "$6" == "--append" ]] || die "unknown argument: $6"
      append_mode="1"
    fi
    if [[ -n "$prompt_out" ]]; then
      write_prompt_metrics "$2" "$3" "$prompt_out" "$append_mode"
      cat "$prompt_out"
    else
      compare_prompts "$2" "$3"
    fi
    exit 0
  fi

  local repos_csv=""
  local queries_csv=""
  local out_dir=""
  local codeatlas_bin="codeatlas"

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repos)
        repos_csv="$2"
        shift 2
        ;;
      --queries)
        queries_csv="$2"
        shift 2
        ;;
      --out-dir)
        out_dir="$2"
        shift 2
        ;;
      --codeatlas-bin)
        codeatlas_bin="$2"
        shift 2
        ;;
      --help|-h)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
  done

  [[ -n "$repos_csv" ]] || die "--repos is required"
  [[ -n "$queries_csv" ]] || die "--queries is required"
  [[ -f "$repos_csv" ]] || die "repos CSV not found: $repos_csv"
  [[ -f "$queries_csv" ]] || die "queries CSV not found: $queries_csv"
  require_cmd "$codeatlas_bin"

  if [[ -z "$out_dir" ]]; then
    out_dir="docs/benchmarks/results/$(date -u +%Y%m%dT%H%M%SZ)"
  fi

  mkdir -p "$out_dir"
  : >"$out_dir/summary.txt"

  collect_repo_metrics "$codeatlas_bin" "$repos_csv" "$out_dir"
  collect_query_metrics "$codeatlas_bin" "$queries_csv" "$out_dir"
  printf "label,file,bytes,lines,estimated_tokens\n" >"$out_dir/prompt_metrics.csv"

  {
    echo "Blog benchmark collection complete."
    echo "Output directory: $out_dir"
    echo "Generated:"
    echo "  - $out_dir/repo_metrics.csv"
    echo "  - $out_dir/query_metrics.csv"
    echo "  - $out_dir/prompt_metrics.csv"
    echo "  - $out_dir/summary.txt"
    echo
    echo "For prompt token comparisons:"
    echo "  bash scripts/blog/collect_blog_metrics.sh compare-prompts <baseline> <with-codeatlas> --out-file $out_dir/prompt_metrics.csv --append"
    echo
    echo "Note: file_count and line_count use git-tracked plus unignored files when the repo is under git."
  } >>"$out_dir/summary.txt"

  cat "$out_dir/summary.txt"
}

main "$@"
