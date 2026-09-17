# Changelog

All notable changes to FOP (Filter Orderer and Preener) are documented in this file.

## [Unreleased]

- Recognise a rule's option list by scanning rather than with `OPTION_PATTERN`. The regex leads with `.*` and cost around 650ns on a rule carrying options, against 15ns for the byte paths — roughly half the cost of checking an added line, for one regex. Behaviour is unchanged — the scan is held to producing the same split as the regex, and agrees on all 609k lines of EasyList and the region lists as well as on the awkward shapes those do not contain. 50ns per rule overall, down from 92ns.
- Flag an unanchored pattern that reads as nothing at all (`fdfdgfgdgfd^`, `fdgfgdfgd`, `fdgfgdfgd$third-party`), with or without a trailing `^`: six or more letters, no digits or punctuation, and not one vowel. It matches that text anywhere in a URL. The bar is deliberately narrow — the form as a whole appears nowhere in 609k lines of EasyList and the region lists, but that only means it is unused, and `doubleclick^`, `prebid^` or `300x250^` are patterns someone could reasonably write. Reported in its own words rather than as a missing `||`, since there is no host to suggest anchoring.
- Say which thing is missing when the rule checks cannot run. fop looks for `.git` in the directory it is given, so running it from a subdirectory found no repository — but the warning blamed git, which was working fine.
- Run the addition checks before the timestamp and checksum passes. Those hash the file body, so removing a line afterwards left the checksum describing content that was no longer there — and with the removal now followed by a commit, that invalid checksum would have been published.
- Leave the flagged lines in place under `--output`, `--output-diff` and `--benchmark`: those ask for a report, and the sorter already writes nothing in that mode. The run still stops at the prompt, so a dry run cannot commit the rules it declined to remove.
- Make `remove-bad-rules` in `.fopconfig` imply `check-rules-on-add`, as the command-line flag already did. On its own it left the checks off and so did nothing at all.
- Restrict the checks to the files `--ignore-all-but` selects, so a run told to touch one file cannot rewrite every list in the repository's diff.
- Treat a diff that cannot be read as a failure rather than as "nothing was added", both before and after removing lines. The earlier code turned it into an empty list, which would have reported a clean bill of health for a check that never ran and could have committed the flagged lines.
- Run the addition checks in sort-only mode (`--no-commit`, `--just-sort`) as well. They were inside the commit flow, so the flags were accepted and silently did nothing; repository detection, which the commit flow also gated, now happens whenever the checks are on. Without a commit to gate there is no prompt — findings are reported, and `--remove-bad-rules` still removes them.
- Flag a host rule that lost its `||` anchor (`rbush.shop^`, with or without options). It is legal and matches the name *anywhere* in a URL, so it also blocks `lampedburbush.shop` and anything else ending in it — over-blocking that stays invisible until a site is reported broken. Reported as advice, never deleted, since the fix is to add the anchor. Deliberate uses barely exist: across 609k lines of EasyList and the region lists there was one, itself a typo (`arketing.indianadunes.com^`, missing its leading `m`).
- Leave a `:has-text()` group alone when it holds something that cannot be folded: a regex carrying flags (`/foo/i`, whose flags would not survive and whose text would become a literal six-character search) or an empty alternative (`/foo|/`, which matches everything, so dropping it narrows the rule). A lone `/` is no longer treated as a regex either — slicing it panicked and aborted the whole sort.
- Merge `:has-text()` rules for `#?#` as well as `##`. A procedural group with the same domains and base selector now folds into one regex the way a hiding group already did, and the two group separately so they are never combined. Exceptions (`#@#`, `#@?#`) and injection separators (`#$#`, `#%#`) are excluded — the scan stops at whichever separator comes first rather than stepping over one it cannot merge, which previously let `#@##ad` split at the `##` those two `#` characters form. Exceptions are excluded because an exception cancels a hiding rule by matching its selector *text*, so folding two of them would leave neither original string in existence and the rules they cancelled would no longer be excepted.
- Deduplicate the alternatives when merging, so a part-merged group does not grow. `/A|B/` plus `A` plus `B` gave `/A|B|A|B/`, and each later run compounded it; splitting is top-level only, so `(a|b)c` stays one alternative.
- Do not merge a nested `:has(span:has-text(x))`. The pattern matches lazily, leaving the base with an unclosed `(` and the argument with an extra `)`, so the rebuilt rule was a bracket short and its regex searched for a literal `)`. Such groups are now left alone.
- Add `--check-rules-on-add` (`.fopconfig`: `check-rules-on-add`) to check newly added lines for rules that cannot work, reported with file and line and prompting before the commit goes ahead. It catches a separator with no selector (`example.com##`), a truncated selector (`##.ad[href="x"`), an option marker with no options (`||example.com$`), an option with no value (`$domain=`) and an unrecognised option (`$thrid-party`). An unrecognised option is matched against the known set by edit distance and reported with a suggestion — `unknown option: thrid-party -- did you mean third-party?` — rather than against a table of known misspellings, so a typo nobody has seen before is still named. An option that is simply new gets no suggestion instead of a wrong one. `--remove-bad-rules` deletes every flagged line instead of prompting and then commits what is left, so a run that adds two good rules and one bad one lands the two; it reports how many were advice rather than defects, since a bare hostname is legal in a plain domain-list file and `ignorefiles` is the way to exclude one. `--ci` still fails only on defects.
- Flag a bare hostname written as a rule (`domain.com`, `anotherdomain.co.nz`) and suggest `||host^`. It is legal syntax — it matches the name anywhere in a URL — but it also matches `notdomain.com.evil.test` and any URL merely mentioning the name, and it is almost never what was meant. Reported as advice, never deleted by `--remove-bad-rules`: in a plain domain-list file a bare host is exactly what belongs there. Across 608k lines of EasyList and the region lists every instance was in such a file, and no genuine filter list carried one.
- The same pass also flags a malformed domain list (`,example.com##`, `a.com,,b.com##`, `exa..mple.com##`), a selector opening on a combinator (`##> div`), and an empty entry in a `|`-separated option value (`$domain=a.com|`). Balancing covers `{}` as well as `[]` and `()`, so AdGuard CSS injection is checked too.
- Run the same checks in `--ci`: with `--check-rules-on-add`, a CI run audits the committed diff (`origin/master`, or `HEAD~1` when HEAD already matches it) and exits 1 on a defect. Advice such as a bare hostname is printed as a `Notice:` and does not fail the build. `ignorefiles` is honoured. Without the flag, `--ci` behaves exactly as before.
- Restrict the addition checks to files fop would sort. They ran on every added line in the repository, so a `$` in a shell script or a workflow (`export PATH=$PATH:/usr/bin`, `run: echo "$GITHUB_SHA"`) read as a network rule with an unknown option — deleted by `--remove-bad-rules`, and a CI failure for a pull request that touched no filter list at all.
- Recognise a rule's options with the same pattern the sorter uses, rather than by taking the last `$` and splitting on every comma. `$removeparam=/^utm$/`, `$replace=/(foo)bar/$1baz/` and comma-bearing `jsonprune=`/`xmlprune=` values were all reported as defects.
- Exempt AdGuard JavaScript injection (`#%#`, `#@%#`) and scriptlet bodies from the selector checks: they are not CSS, so an apostrophe in a comment is not an unbalanced quote.
- Add `inline-font`, `beacon`, `mp4`, `noop` and `queryprune` to the known options. The omission was harmless while an unknown option was only a warning; with the addition checks it deleted valid rules.
- Resolve the CI base from the remote's default branch instead of assuming `origin/master`, and fail when no base or diff can be resolved. On a `main` repository, or a shallow clone missing `HEAD~1`, the diff failed silently and the audit passed having checked nothing.
- Preserve CRLF line endings when `--remove-bad-rules` rewrites a file, skip the write entirely when nothing matched, and continue past an unreadable file instead of abandoning the rest.
- Do not abort the commit when every finding is advice: `--remove-bad-rules` had nothing to remove, printed `Removed 0 line(s)` and returned, leaving no way to commit a bare-domain addition.
- Fix both `--ci` audits running `git` in fop's own working directory rather than the repository they were pointed at, so `fop --ci <path>` inspected the wrong tree unless it happened to be launched from inside that repo.
- These checks run only on added lines, never over whole files: with the author present a false positive costs a glance, so they can be stricter than `malformed_rule_reason`, which must only ever match rules that are impossible. Measured at zero false positives over 609k lines of EasyList and the region lists.
- Collapse the unrecognised-option check from a chain of ~29 `starts_with` comparisons to a single hash lookup against a new `KNOWN_OPTION_PREFIXES` set, shared with the new addition checks rather than copied.

## [5.5.0] - 2026-09-13

- Add `--ignore-line-minimum` (`.fopconfig`: `ignore-line-minimum`, also a per-file override) to keep rules shorter than three characters. They are dropped as `malformed rule (too short)` by default, which is right for truncation debris but deletes a short rule an author wrote on purpose. The flag lifts only the length floor: a line starting with `"`, `)`, `]` or `}` is still dropped as debris.
- Move the `#@#` -> `#@?#` exception-separator promotion out of `--abp-convert` and into a new `--adguard-convert` (`.fopconfig`: `adguard-convert`), off by default. `#@?#` is AdGuard's spelling — uBO writes the same rule as plain `#@#` — so emitting it was never part of converting ABP selectors to uBO form, and it fired on rules that had nothing for `--abp-convert` to convert. `--abp-convert` alone no longer rewrites an exception separator; pass `--adguard-convert` (with or without it) to get the old behaviour. The `##` -> `#?#` hiding promotion moves with it, so `--abp-convert` now only renames ABP operators and never rewrites a separator.
- Add `--no-commit-mask` to turn masking off when `.fopconfig` sets a level. `--commit-mask=0` cannot: 0 falls through to level 1 by design, so a config-set level had no command-line off switch.
- Respect `--quiet` (but not `--limited-quiet`, which only suppresses the directory listing) when a pre-commit pull fails. The suggested-fix block was printed unconditionally, so CI logged it on every transient failure.
- Fix the stray-branch recovery advice losing commits. It cherry-picked only `HEAD` and then ran `git branch -D`, so anyone following it with more than one unpushed commit on the branch lost the rest. Now resolves the repository's actual default branch rather than assuming `master`, shows the full commit range, and cherry-picks all of it. The ranges are remote-qualified (`origin/main..<branch>`), since the default branch is read from `refs/remotes/origin/HEAD` and a CI clone may have no local branch of that name — and a stale local one would re-apply already-pushed commits. On the default branch itself — or when no default branch resolves at all, where every suggested range would be `bad revision` — the advice is now just the `--set-upstream` route, instead of a no-op cherry-pick ending in a `git branch -D` that git refuses. The remote is resolved rather than assumed to be `origin`.
- Fix the commit URL for hosts carrying a port or userinfo. `bitbucket.org:443` and `git@bitbucket.org` missed the Bitbucket check and got the singular `/commit/` template, and an `ssh://` remote was printed verbatim (`ssh://git@host/u/r/commits/<sha>`) rather than as a link. The scp form is recognised for any user, not just `git@`, so deploy-key remotes (`deploy@host:u/r`) are linkified too, and `git+ssh://` normalises like `ssh://`. Schemes with no web equivalent (`git://`, `file://`) are left untouched rather than mangled.
- Never print remote credentials. A remote of the form `https://x-access-token:TOKEN@host/u/r`, common in CI, had its token echoed into the `Commit successful:` line. `http://` remotes are covered as well — a self-hosted host on plain http is exactly where an embedded token lives — and userinfo is anchored on the last `@` that is actually followed by a host, so a password containing `@` or `/` no longer leaves a fragment of itself in the printed URL. The same normaliser now backs the `Create PR at:` line, which kept a weaker private copy that passed an https remote through verbatim.
- Treat a URL with userinfo as its host when checking `--commit-mask` exemptions, so `https://git@github.com/...` stays unmasked like every other github.com link.

## [5.4.0] - 2026-09-01

- Add `--commit-mask=N` to defang URLs in commit messages
  - Levels: `1=[.]`, `2=(.)`, `3=space`, `4=preserve subdomain dot (mask only eTLD+1)`, `5=Unicode U+2024 lookalike`
  - Unknown levels fall through to level 1
  - `github.com` and `gitlab.com` (and subdomains) always exempt so PR/issue links stay clickable
  - Compound TLDs handled for ~25 country codes (`co.uk`, `co.nz`, `com.au`, `co.jp`, `com.ng`, `com.hk`, `co.il`, `com.vn`, etc.)
- Add `--commit-mask-users=u1,u2` to restrict masking to specific `git user.name` values
- Add `--commit-mask-bare` to also mask hostnames without `http(s)://` (opt-in; FP risk on filenames)
- Add `--commit-mask-exempt-hosts=h1,h2` for self-hosted Gitea/Forgejo/private GitLab instances. Adds to the built-in `github.com` / `gitlab.com` / `codeberg.org` list.
- Add `--commit-url-template=TMPL` to override the `Commit successful:` URL builder. Placeholders `{base}` and `{sha}`. Default is auto-picked per host: bitbucket.org gets `{base}/commits/{sha}` (plural), everything else gets `{base}/commit/{sha}`.
- Warn at startup if `--commit-url-template` is set but missing `{sha}` (catches typos like `{shA}`).
- Display `Commit message (masked):` label only when masking actually changed the text
- Default `rebase-on-fail` to true; auto-recover from `[remote rejected] cannot lock ref` races between commits
- Add `--no-rebase-on-fail` to opt out of auto-rebase
- Print actionable suggested-fix commands when push fails (rebase conflicts, no upstream branch)
- Stop removing valid three-character rules. The minimum rule length was raised to 4 to catch garbage like `"])`, which also deleted `##a`, `*/*` and `/a/` from list files and reported them as malformed. The floor is back to 3 (counted in characters, so a lone multi-byte character or a stray BOM is still dropped), and lines starting with `"`, `)`, `]` or `}` are rejected instead — no filter syntax begins with a closing bracket or a quote.
- Cap the worker pool at 8 threads (override with `RAYON_NUM_THREADS`). Work is parallelised one file per task, but the pool was sized to the core count regardless of the workload, and each worker holds its own allocator heap. Peak memory on a 32-core machine: 75 -> 27 MB for a single small file, 274 -> 115 MB for 670k lines across 270 files, at equal or better speed.
- Fix `--commit-mask=4` leaking part of the registrable domain when a URL's compound TLD is uppercase. The eTLD lookup was case-sensitive against a lowercase table, so `www.Example.CO.UK` masked only the final dot (`www.Example.CO[.]UK`) instead of the whole eTLD+1 (`www.Example[.]CO[.]UK`).
- Support both spellings of an options-only rule, keeping whichever is written. `*$ping,third-party` was being trimmed to `$ping,third-party`; the `*` is the rule's pattern and the pattern-less form isn't accepted by every consumer of these lists. A bare `$ping,third-party` is still left bare, so existing rules are not rewritten. Applies to `@@*$…` too, and a repeated wildcard (`**$ping`) collapses to one rather than being dropped.
- Fix `$@$` and `#@?#` rules losing selector whitespace. They were missing from the element-rule check, so `example.com$@$script[tag-content="ad config"]` became `"adconfig"` while the identical `$$` form was left alone.
- Fix a rule of nothing but wildcards (`***`) being written back as a blank line.
- Fix silent corruption of AdGuard cosmetic rules. The `$` inside a cosmetic separator was read as the start of a filter-option list, so the `$option.option` typo fix rewrote every `.` in the selector and stylesheet body to `,` — `example.com#$#div.ad { display: none; }` became `div,ad`, which still parses but matches every `div` *or* every `ad` element. Affected `#$#`, `#@$#`, `#$?#`, `#@$?#`, `$$` and `$@$` (not `##`, `#?#` or `#@#`), and `--parse-adguard` avoided it.
- Fix crash on non-ASCII input. A CJK comment header (`! 日本語です`) aborted the run, as did an internationalised (IDN) host in a commit message or git remote.

## [5.3.0] - 2026-03-18

- Add per-file config sections in `.fopconfig` with `[filename]` overrides
- Add `hls=`, `xmlprune=`, `tag=` to recognised filter options
- Add version bump type selector (patch/minor/major) to publish workflow
- Show commit URL and message after successful push
- Overwrite "Connecting to server" and "Comment accepted" lines on success
- Warn when Windows line endings (CRLF) detected, suggest `.gitattributes` fix
- Update GitHub Actions to Node.js 24-compatible versions
- Update README-WIN.md with line endings guide

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
