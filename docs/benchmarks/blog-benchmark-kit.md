# Blog Benchmark Kit

This document defines a repeatable benchmark and evidence-collection workflow
for a CodeAtlas launch or feature blog post.

It is intentionally practical: run the commands, collect the generated CSV
files, and turn them into tables/charts for the article.

## Goals

Use this kit to collect evidence for:

- multi-repo service ergonomics
- indexing scale and incremental refresh value
- query latency and repo-scoped correctness
- semantic quality where adapters are available
- prompt-size and token-savings impact for AI workflows

## Recommended Repo Matrix

Use 4-6 repositories with distinct characteristics:

- Rust-heavy repo
- TypeScript repo with `tsserver` available
- Kotlin repo with JVM bridge available
- mixed-language application repo
- medium/large service or app repo
- repo with frequent small edits for incremental-refresh demos

Keep the matrix stable across reruns so results are comparable.

## Artifacts

The benchmark kit uses these files:

- repo manifest: `docs/benchmarks/templates/blog-repos.tsv`
- query manifest: `docs/benchmarks/templates/blog-queries.tsv`
- collection script: `scripts/blog/collect_blog_metrics.sh`
- prompt templates: `docs/benchmarks/templates/prompts/`

Legacy `.csv` templates may also exist in the same directory for backward
compatibility, but the `.tsv` files are the recommended templates.

The script writes CSV outputs under a timestamped directory.

## Metrics To Capture

### Repository-level metrics

- repo id
- source path
- total file count
- total line count
- git commit SHA (if present)

### CodeAtlas indexing metrics

Collected from `codeatlas quality-report`:

- files discovered
- files parsed
- files errored
- symbols extracted
- semantic symbols
- syntax symbols
- semantic coverage percent
- avg confidence
- files with semantic support
- semantic win rate
- wins, losses, ties
- final KPI result (`PASS` / `FAIL`)

### Query timing metrics

For each repo/query pair:

- command type (`search-symbols`, `get-symbol`, `file-outline`)
- target repo
- query input
- exit status
- wall-clock seconds

### Incremental workflow metrics

For at least one repo per language:

- full index wall-clock time
- `repo refresh --git-diff` wall-clock time after a 1-file edit
- `repo refresh --git-diff` wall-clock time after a 10-file edit

These can be collected with the same script by rerunning after edits.

### Token-savings metrics

Compare two prompts for the same task:

1. Baseline prompt: includes raw code or large file excerpts.
2. CodeAtlas prompt: includes concise CodeAtlas outputs such as symbol IDs,
   file outline, repo outline, and a shorter task description.

Record:

- prompt label
- byte count
- line count
- estimated token count

## Token-Savings Methodology

Token counts depend on the model tokenizer. This kit uses a simple, explicit
estimate so you can compare prompt size consistently even if you switch models.

Default estimate:

- estimated tokens = `ceil(bytes / 4)`

This is not exact tokenizer parity. It is a stable approximation suitable for a
blog post if you label it clearly as an estimate.

If you want exact model-specific counts later, you can swap in a tokenizer
tool, but keep the baseline/with-CodeAtlas prompts unchanged.

## Prompt Comparison Design

For each AI workflow example, create a pair of prompt files:

- `baseline.md`
- `with-codeatlas.md`

Good candidate tasks:

- "Explain how request routing works in this repo."
- "Find the service startup path and health endpoint."
- "Show me where a symbol is defined and how it is used."
- "Summarize the repo structure relevant to billing/auth/search."

### Baseline prompt shape

- task request
- pasted file contents or large excerpts
- optional note asking the model to inspect relationships manually

### With-CodeAtlas prompt shape

- same task request
- `repo_id`
- `repo-outline` output
- `file-outline` output for one or two files
- exact symbol IDs from `search-symbols` / `get-symbol`
- only minimal raw code, if any

The prompt pair should answer the same question with different context shapes.

## Suggested Blog Structure

### 1. What CodeAtlas changes

- one persistent local service
- one shared store for many repos
- one MCP bridge config for AI clients

### 2. What the benchmarks show

- indexing coverage and quality
- query speed
- incremental refresh wins
- token savings for AI prompts

### 3. Where it helps most

- multi-repo developers
- AI-assisted exploration and symbol lookup
- repos with strong TypeScript or Kotlin semantic coverage

### 4. Honest boundaries

- local-first, not hosted
- exact token counts vary by model
- semantic quality depends on adapter availability

## Example Claims You Can Back With Data

- "One CodeAtlas service handled N repos with one MCP configuration."
- "Semantic coverage reached X% on repo Y."
- "A repo refresh after a small edit was X times faster than a fresh index."
- "The CodeAtlas-assisted prompt was X% smaller by estimated token count."
- "Symbol lookup and file-outline queries stayed below X ms on repo Y."

## Run Flow

1. Fill in `blog-repos.tsv`.
2. Fill in `blog-queries.tsv`.
3. Run:

```bash
bash scripts/blog/collect_blog_metrics.sh \
  --repos docs/benchmarks/templates/blog-repos.tsv \
  --queries docs/benchmarks/templates/blog-queries.tsv
```

4. For prompt savings, create prompt files and run:

```bash
bash scripts/blog/collect_blog_metrics.sh compare-prompts \
  prompts/baseline.md \
  prompts/with-codeatlas.md \
  --out-file docs/benchmarks/results/<timestamp>/prompt_metrics.csv \
  --append
```

## Output Files

The script writes:

- `repo_metrics.csv`
- `query_metrics.csv`
- `prompt_metrics.csv`
- `summary.txt`

When a repository is under git, the inventory counts use git-tracked plus
unignored files. This aligns more closely with what CodeAtlas is likely to
discover than a raw filesystem walk.

You can append multiple prompt pairs into one `prompt_metrics.csv` by reusing
the same output path with `--append`.

## Manifest Format

Use TSV for the manifests by default. This avoids the quoting problems of
shell-level CSV parsing when notes contain commas.

The script still accepts the older simple CSV templates for backward
compatibility, but TSV is the recommended format.

Use these directly for charts, tables, and supporting evidence in the blog.
