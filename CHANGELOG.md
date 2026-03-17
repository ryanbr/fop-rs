# Changelog

All notable changes to FOP (Filter Orderer and Preener) are documented in this file.

## [5.2.2]

- Show commit URL after successful push
- Fix commit URL SSH-to-HTTPS conversion
- Overwrite "Connecting to server" line with commit URL on success
- Warn when Windows line endings (CRLF) detected, suggest `.gitattributes` fix
- Update GitHub Actions to Node.js 24-compatible versions
- Update README-WIN.md with line endings guide
- Update README.md options table

## [5.2.1] - 2026-03-17

- Add `--benchmark` flag for timing sort performance (3 iterations, reports lines/sec, MB/sec, ms/file)
- Support `--benchmark` with `--check-file` for single file benchmarking
- Fix `jsonprune=` filter parsing: escaped `\$` separator, dot replacement, and space preservation
- Add `mimalloc` allocator for improved parallel allocation performance
- Add PGO (profile-guided optimization) to Linux v3/v4 and macOS ARM CI builds
- Add `parsing.md` documenting filter rule support across ABP, uBO, and AdGuard
- Use `Cow<str>` in `remove_unnecessary_wildcards` to avoid allocation on common path
- Use `Cow<str>` in `filter_tidy` preprocessing to avoid allocation when unchanged
- Fast reject for `is_extended` check in `element_tidy` using match on separator
- Remove duplicate `--add-timestamp=` match arm in CLI parser
- Extract `is_localhost_file`/`is_adguard_file` helpers to reduce duplication

## [5.2.0] - 2026-03-10

- Fix sorting of `-abp-properties` rules

## [5.1.0] - 2026-03-10

- Add `--abp-convert` to convert `-abp-contains`/`-abp-has` into `has-text`/`has`
- Offer v4 AVX optimised builds
- Add security warning information to README
- Suppress intentional clippy `writeln!` warning for Unix line endings

## [5.0.3] - 2026-03-09

- Fix forced Unix line endings (LF) in output files

## [5.0.2] - 2026-02-27

### Features
- Add `--parse-adguard` for improved AdGuard rule parsing (global and per-file)
- Add `--localhost-files=` for specifying specific localhost format files

### Performance
- Replace regex with string ops for localhost sort key extraction and entry validation
- `remove_unnecessary_wildcards` — skip allocation when no wildcards
- `filter_tidy` — avoid allocation when no spaces
- Optimize `add_checksum`: return written checksum, skip redundant verify read
- Avoid mutex lock on every `write_warning` call

### Fixes
- Trim leading/trailing whitespace from element rule selectors
- Adjust typo checks
- Fix clippy warning

## [5.0.1] - 2026-02-10

### Features
- Implement `validate_checksum` functionality
- Add `--validate-checksum-and-fix` option
- Have `--add-checksum` automatically validate the checksum
- Separate datestamp functions into `fop_datestamp.rs`

### Performance
- Avoid redundant String allocations for regex captures in `combine_filters`
- `parse_bool`: trim whitespace and avoid allocation
- `Args::parse`: collect args once instead of iterating `env::args()` multiple times
- Avoid allocating a lowercase copy of the string on every call
- Release mutex locks before file I/O in `flush_warnings`

### Fixes
- Fix regex for combining rules
- Improve `fop_checksum.rs` compatibility

## [5.0.0] - 2026-02-06

### Features
- Add `--add-checksum=<files>` to insert checksums on commit
- Add `--add-timestamp` to update timestamp in file headers
- Add `--git-binary=<path>` for custom git location
- Check for `#+js` for missing `#` typo
- Check for invalid characters in `domain=`

### Performance
- `String` → `Cow<'static, str>` — avoid heap-allocating literals
- `check_additions` — return references instead of cloning
- Replace `LEADING_COMMA` regex with `trim_start_matches`
- `fix_all_typos` — avoid cloning when no typos
- `EXTRA_HASH` — build result from captures instead of running the regex twice
- Implement fast reject for filter processing
- `TRIPLE_DOLLAR` and `DOUBLE_DOLLAR` don't need regex
- Replace O(n) `remove(0)` loop in `remove_unnecessary_wildcards` with single slice
- `format_version_utc` — compute directly instead of format-then-parse

### Fixes
- Remove any excess spaces on network rules
- Prune spaces in domain cosmetics
- Improve banned domains functionality
- Remove dead code, fix `check_additions`
- Improve mutex usage pattern in `write_warning`
- Simplify `flush_warnings` with early returns

## [4.3.2] - 2026-01-27

- Switch from `colored` to `owo-colors` dependency
- Add `--limited-quiet` to suppress directory listing output
- Ensure `A:`/`P:` commit prefixes are followed by a URL
- Include CLAUDE.md project guide
- Refactor `fop_git` for limited git output with `limited_quiet`

## [4.3.1] - 2026-01-21

- Fix `--ci` argument detection
- Allow `--ci` to properly check banned domains in PRs and direct commits
- Support commit comment `--history` from the CLI
- Early return when no history (skip unnecessary loop iteration)

## [4.3.0] - 2026-01-19

### Features
- Add `--ci` mode for GitHub Actions checks (exit codes on failure)
- Implement `rustyline` for better user input with history
- Support `--only-sort-changed` to only process git-changed files
- Add `--rebase-on-fail` to auto-rebase and retry failed pushes

### Performance
- Avoid allocating a new String for every input line
- Pre-allocate HashSet capacity for duplicate detection
- `std::mem::take()` to swap fields without cloning
- Don't hold `WARNING_OUTPUT` lock while writing the file
- Use `std::fmt::Write` and iterate HashSet directly

### Fixes
- Fix a regex parser issue
- Code refactoring with `with_tracked_changes`

## [4.2.9] - 2026-01-18

- Ensure banned-list isn't checked against itself
- Collect duplicates locally, merge once (reduces lock contention)
- Optimize `element_tidy`: collect regex matches before iteration
- Move `SKIP_SCHEMES` to module-level const

## [4.2.8] - 2026-01-17

- Add `--check-banned-list=` to detect banned domains in git additions
- Add `--auto-banned-remove` to auto-remove banned domains and commit
- Ignore additions with `domain=` and `from=` for banned list checks
- Optimize banned domains function

## [4.2.7] - 2026-01-16

- Add `--pr-show-changes` to include rule changes in PR body
- Fix ARM Windows build (avoid VCRUNTIME140 DLL dependency)
- Pre-allocate HashMap capacity for config
- Clear global `SORT_CHANGES` after processing
- Expand typo examples
- Fix ASCII chars and ensure URL is within limit

## [4.2.6] - 2026-01-14

- Allow merging of `:has-text()` on same elements
- Add `--only-sort-changed` to only sort git-changed files
- Allow user to `git restore` on cancelled large commits
- Support `ABORT` to instant revert changes
- Reduce mutex contention in parallel diff collection
- Improve user reporting when a git push fails

## [4.2.5] - 2026-01-10

- Improve git branch name detection with `--create-pr`
- Make `fop_git` work with GitHub, GitLab, and self-hosted git
- Prefer non-master/non-main branches when using `--create-pr`
- Refactor `fop_git.rs` for better remote detection

## [4.2.4] - 2026-01-08

- Add `--ignore-all-but=` to only process specific files
- Add `#[inline]` to hot path functions
- Use `any()` iterator instead of manual loops

## [4.2.3] - 2025-12-31

### Performance
- Optimize localhost sorting with cached key computation
- Optimize element filter sorting with cached key
- Use `to_ascii_lowercase()` for option names
- Use cached `file_type` from walkdir to reduce syscalls
- Optimize typo detection: single regex pass instead of two
- Optimize sort: zero-allocation ASCII case-insensitive comparison

### Fixes
- Fix argument parsing of `--ignore-dot-domains`
- Fix duplicate `base_cmd` args in `create_pull_request`
- Fix line number tracking in diff parser for typo detection
- Handle detached HEAD in branch detection
- Make domain sorting deterministic (non-inverted before inverted)

## [4.2.2] - 2025-12-30

- Sync `package-lock.json` version and add win32 platform

## [4.2.1] - 2025-12-30

- Add `--output` for changed file output
- Allow multiple file output with empty `--output-diff`
- Add Linux RISC-V build
- Improve compatibility for Linux ARM and Windows IA32
- Add Visual Studio Code integration docs

## [4.2.0] - 2025-12-28

- Replace `once_cell` with `LazyLock` (stdlib)
- Remove `dirs` dependency
- Optimize: skip regex parsing for filters without options
- Optimize: avoid double file read using Cursor
- Add `direct-push-users` to bypass PR for trusted users
- Fix Windows ARM building

## [4.1.2] - 2025-12-27

- Create optimized Linux/Windows ARM binaries
- Add domain typo detection (domain separator typos)
- Add `create-pr = true` config option
- Fix typo dry-run bug
- Add warnings for incompatible options
- Fix regex errors in tests

## [4.1.1] - 2025-12-17

- Add `--quiet` to limit verbose output
- Add `--output-diff=` for diff output without modifying files
- Add `--check-file=` for sorting a specific file
- Add `--ignore-config` to skip `.fopconfig`
- Support `--fix-typos` scanning improvements
- Add Windows/Linux ARM builds

## [4.1.0] - 2025-12-15

- Separate git functions into `fop_git.rs` module
- Add `--create-pr` for creating pull request branches
- Add `--fix-typos` for cosmetic rule typo detection
- Use `--rebase` to avoid merge branch commits
- Optimize git commands with `.args()` chaining

## [4.0.4] - 2025-12-13

### Features
- Support ABP `#0` rule parsing
- Support custom `addheader=` flag
- Support additional AdGuard custom rules

### Performance
- Avoid `clone()` in main loop — use `into_owned()`
- Use `&str` comparisons instead of `String` for sorting
- `sort_unstable_by` for 10-20% faster sorting
- Skip combine loop overhead for single-filter sections
- Use AHashMap with pre-allocation for config

## [4.0.3] - 2025-12-12

- Fix Windows npm builds

## [4.0.2] - 2025-12-12

- Validate `--localhost` rules and remove invalid entries
- Refactor into single `process_location` call

## [4.0.1] - 2025-12-12

- Add `--ignore-dot-domains` option
- Add `disable_domain_limit` and `warning_output` support
- Optimize warning output with buffered writes
- Avoid repeated string allocations in domain validation
- Enable Fat LTO optimization
- Target `apple-m1` for macOS ARM builds

## [4.0.0] - 2025-12-11

- Add `--no-large-warning` option
- Add `--file-extensions=` for configurable file extensions
- Add `--comments=` for configurable comment prefixes
- Add missing uBO options

## [3.9.14] - 2025-12-10

- Fix npm publish workflow

## [3.9.13] - 2025-12-10

- Add missing filter flags
- Ensure users get the latest version on install

## [3.9.12] - 2025-12-10

- Add `--ignoredirs=` option ([#1](https://github.com/ryanbr/fop-rs/issues/1))
- Refactor `main.rs` into `fop_sort.rs` and `tests.rs`
- Add `--show-config` option
- Remove EasyList-specific hardcoded rules

## [3.9.11] - 2025-12-09

- Add macOS build support
- Fix npm release issue

## [3.9.9] - 2025-12-09

- Add `--git-message` for non-interactive commit messages
- Add Windows README

## [3.9.8] - 2025-12-09

- Add `--localhost` for hosts file sorting
- Add `--ignorefiles=` option
- Add `.fopconfig` configuration file support
- Add `--config-file=` for custom config path
- Implement `ahash` for faster HashSets
- Use BufWriter for faster file writing
- Add colored terminal output

## [3.9.7] - 2025-12-09

- Add `--disable-ignored` option
- Add `--no-sort` option
- Remove hardcoded `easylist_adservers.txt` reference
- Make FOP.py-compatible sorting the default

## [3.9.6] - 2025-12-08

- Add `--no-msg-check` option
- Add missing selectors
- Build both baseline and optimized x86_64 binaries

## [3.9.5] - 2025-12-08

- Improve support for extended AdGuard and ABP rules
- Fix underscore replacement to only affect option names, not values

## [3.9.4] - 2025-12-07

- Add support for uBO rules
- Improve support for regex, denyallow, removeparam
- Add npm publish workflow

## [3.9.3] - 2025-12-08

- Initial Rust port of Python FOP
- Add parallel file processing with Rayon
- Simplify `remove_unnecessary_wildcards`
- Extract `sort_domains` helper function
- npm package setup
