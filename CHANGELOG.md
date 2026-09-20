# Changelog

All notable changes to FOP (Filter Orderer and Preener) are documented in this file.

## [Unreleased]

- Stop reading an AdGuard HTML filter's selector as an option list. `$$` and `$@$` open a selector, but in the default mode -- where those separators are not parsed as cosmetic -- what followed was taken for options, and FOP warned that `amp-consent` in `m.timesofindia.com,…$$amp-consent` was an option it did not know. The rule was written back untouched, so only the warning was wrong, and no flag could silence it: `--parse-adguard` sorts such a rule as cosmetic, but a list holding both kinds cannot always be sorted that way. Any line carrying a cosmetic separator is now left alone here. An unknown option on a network rule still warns; on the corpus only the two `$$` warnings disappear, and output is byte-identical across test-lists in both modes and the whole corpus. The test is loose in one direction: a network rule whose pattern holds `$$` keeps its options in the order written, rather than sorted. Adblock syntax gives no way to write that rule -- the first `$` opens the option list -- and none appears in 1.67M lines of real lists.
- Read git's push and pull messages in English whatever the user's language. FOP tells a lost push race (`fetch first`, `non-fast-forward`, `Updates were rejected`), a conflict (`CONFLICT`, `could not apply`) and a missing upstream apart by their text, which git translates under a non-English locale; there a lost race could read as some other failure and get one retry instead of five. The pull after committing, the retry's pull and both pushes now run with `LC_ALL=C`; every other git command keeps the user's language.
- Close five ways a repository could make FOP act against the user running it, as when sorting a checked-out pull request. A `.fopconfig` in the working directory could name the program run as git (`git-binary = ./script`, which then ran), or point `warning-output` or `output-diff` at any file, which FOP overwrote. From there `git-binary` is now ignored, `warning-output` must stay inside the directory -- judged with its folders resolved, since `logs/` could itself be a symlink out -- else warnings go to stderr, and an `output-diff` outside it is an error rather than silently becoming a real sort; the command line, `~/.fopconfig` and `--config-file` are unaffected. A symlink planted at a list's `.temp` or `.backup` name made the sort write over its target: every file FOP creates -- temp, backup, `--changed`, `.diff`, warnings -- now refuses a symlink and is created exclusively, and a list whose backup cannot be made safely is left unsorted. A list that was a symlink to a file outside the tree was read and rewritten into the repository as a regular file, ready for `commit -a` to publish; such links are now skipped with a warning, while links between lists in the tree still work. Output is byte-identical across 374 files in every mode, and speed is unchanged.
- Stop at a conflict in the pull after committing, and exit 1 whenever a commit could not be published. When another push had edited the same lines, the `git pull --rebase` after the commit stopped mid-rebase, but FOP only warned that the pull had failed and pushed anyway: from a detached HEAD that failed, then the retry's pull failed on the unmerged files, and the advice never mentioned a conflict. It now says so, pushes nothing, and prints how to finish the rebase. A rebase conflict in the retry, a push that still failed, and a push failure with `--no-rebase-on-fail` all exited 0, so a script driving FOP could not tell its commit had not landed; they now exit 1, once the run is otherwise complete. The retry's check for an unresolved merge also no longer says "nothing was committed", since by then the commit exists.
- Explain a push that lost a race, and retry it until it lands. When another push reaches the branch between FOP's pull and its push, git rejects it (`fetch first`, `non-fast-forward`, or GitHub's `cannot lock ref ... is at X but expected Y`). `--rebase-on-fail` already rebased and pushed again, but git's raw rejection was printed first and, under `--limited-quiet`, the retry's own messages were suppressed along with the directory listing, so a commit that had been published read as lost. FOP now says what happened ("Push rejected: the remote branch moved on ... Rebasing and retrying"), shows the outcome and the commit link unless `--quiet` is set, and tries up to five times, since a busy branch can win the race again. Any other push failure is still shown as git reports it.
- Take a checksum over a list exactly as it stands. FOP appended a final newline before hashing, while ABP's reference script hashes the file as it is, so for a list that does not end in a newline the two disagreed and `--validate-checksum` computed a value no other validator would. A file FOP has sorted always ends in a newline, so this showed when checking lists FOP did not write; files ending in one are unaffected. eyeo's `exceptionrules.txt`, `abp_anti_cv.txt` and `adblock_premium.txt` all end without one; FOP now computes for them what the reference script does. Their published checksums still do not validate, because eyeo computes them with the `! Version:` line left out.
- Make `--benchmark` time the sort, and only the sort. It ran as a dry run, which writes each sorted file out, reads it back and builds a full unified diff against the original; the diffs were never used, but on a heavily reordered list building one took far longer than the sort -- AdGuard's 166k-line base list ran for over 20s a pass, against 0.17s to sort. A benchmark run now stops once the file is sorted. It also wrote to the lists it was measuring: `--add-timestamp` and `--add-checksum`, from the command line or `.fopconfig`, still ran after each pass. So did they under `--output-diff` and `--output`, which promise to leave the files untouched; no dry run writes them now. The timing itself: one untimed warm-up run absorbs regex compilation and thread start-up, then N runs (`--benchmark=N`, default 5, where it was a fixed 3), reported as median, min and max with throughput from the median, alongside the thread count. The checks on git additions no longer run inside it, and each warning is printed by the warm-up alone rather than once per run.
- Keep an escaped comma inside a uBO scriptlet argument. `\,` is a comma within an argument, but the `+js()` spacing fix split the arguments on every comma, so an escaped one gained a space and the argument changed. `trusted-set-local-storage-item, cookieConsent, necessary\,preferences` stored `necessary\, preferences`; `rpnt`, `nostif` and `m3u-prune` rules had spaces put into their regexes and needles. 18 rules in the uBO snapshots were altered, in every release since 4.3.2. `--convert-trusted` now splits the same way, so a `\,` in a cookie's name is no longer read as the start of its value. Escapes are counted as uBO counts them — an odd run of backslashes escapes the comma, so `\\,` is an escaped backslash before a real separator — for option values too, and a rule quoting an argument in backticks, the third quote uBO accepts, is left as written like one using `"` or `'`.
- Keep the rule an AdGuard hint applies to directly below it. `!+ PLATFORM(...)`, `!+ NOT_PLATFORM(...)` and `!+ NOT_OPTIMIZED` apply to the next line only, but a hint is a comment, so the rules below it were sorted as a section, and another rule could move under the hint. In AdGuard's own lists 30 of 2,515 hints ended up over a different rule. One was an exception limited to iOS and Safari: `gpt.js` moved out from under it, so it applied on every platform, and `pagead/managed/` took its place. The rule after a hint is now tidied as usual but neither sorted nor merged, and the rest of its section sorts as before. A chain of hints or a blank line still targets the next rule; any other comment ends the hint.
- Keep the arguments of `:matches-property()`, `:matches-css-before()` and `:matches-css-after()` as written. They are regexes, but they were tidied as selectors, so `/__adv+/` became `/__adv + /`, as happened to `:contains()` before. No rule in the current snapshots was affected.
- Stop `--pr-show-changes` recording every domain merge in full. Each pairwise step was kept with both rules and the merged line, which on Eyeo's `exceptionrules.txt` means hundreds of copies of lines growing past 500k characters — 372 MB at peak against 98 MB without the setting — and it kept that mode on the step-by-step merge, at 6.2s. The PR description lists 40 and counts the rest, then cuts the whole text to 1,700 characters, so almost none of it was ever used. The steps it can list are now kept in full and the rest only counted, merged in one pass: 0.94s and 99 MB with the setting on, level with it off, and the description reads as before, its "… and N more" drawn from the count.
- Merge a run of rules sharing a selector or pattern in one pass. Each rule was tried against the merged line so far, re-parsing it, rebuilding its domain set and re-sorting it every time, so the cost grew with the square of the group: Eyeo's `exceptionrules.txt`, whose Acceptable Ads rules carry thousands of domains, took 6.2s, nearly all of it merging. A run now gathers its domains once and sorts and joins them at the end: 0.83s, and peak memory from 147 MB to 96 MB. Output is byte-identical — across 374 files in the default, `--alt-sort` and `--parse-adguard` modes — and a fuzz test holds both paths to the previous implementation, kept verbatim in the tests. A rule whose domains could make the shortcut differ, such as one holding `#`, `$` or `=`, or one also named in its own options, still takes the pairwise step, as do the steps `--pr-show-changes` lists in the PR description, which are recorded one at a time.
- Stop refreshing a timestamp over a rule that mentions one. Any line holding `Last modified:` or `Last updated:` counted as the timestamp line, so with `--add-timestamp` a rule such as `example.com##div:has-text(Last updated:)` was replaced by `! Last updated: <now>` and lost — while sorting, within the first ten lines of any section, since the window restarts after each comment; and when adding one to a list without a timestamp header, anywhere in the file. Only comments now qualify: `!`, or `#` not opening a cosmetic separator (`##`, `#@#`, `#?#`, `#$#`, `#%#`), plus a hosts banner such as `## Last updated:` when the timestamp comes first, since no selector begins with that text. Header timestamps refresh exactly as before across the test-lists snapshots and a 1.67M-line corpus.
- Stop the `$option.option` typo fix rewriting cosmetic rules whose domain list mixes plain and regex domains. A regex domain ends in `$/`, which read as an option marker, so uAssets' `…(?:com|xyz)$/##+js(acs, Math.random, …)` became `Math,random`, splitting the scriptlet's argument. The fix now applies only to lines with no cosmetic separator, which is how ABP, uBO and AdGuard themselves decide a line is a network rule. The narrower test it replaces, introduced in 5.4.0, existed to repair a network rule carrying `##` in its URL path; no such rule can match anything, and none appears in 2.2M lines of real lists, so a line like that is now left as written.
- Keep the spaces in a regex network rule that carries options. Only a regex with no options was recognised, so AdGuard's `…([a-z]+=[^&=? ]*&)*id=…/$script,third-party` had its character class rewritten to `[^&=?]`, which also matches spaces. The same held for `@@/…/` exceptions. Both in 5.5.0 as well.
- Keep the spaces in an AdGuard `$extension=` value. It names a userscript exactly, and the sort stripped its spaces, so `@@||usanetwork.com^$extension='AdGuard Assistant'` became `'AdGuardAssistant'`, which names nothing: the exception silently stopped applying. Seven rules in AdGuard's allowlist were affected, in 5.5.0 as well.
- Treat an empty path option as unset. An empty value was read as the path `""`, so the option counted as present with an unusable path: a bare `warning-output =` — as the sample configuration in the README ships it — sent every warning to a file that was never written, so none were shown; an empty `output-diff` switched on dry-run and nothing was sorted; and an empty `check-banned-list` warned on every run that it could not load its list. This affected `warning-output`, `output-diff` and `check-banned-list` in `.fopconfig`, in 5.5.0 as well. On the command line an empty value is now an error, exit 2, rather than being dropped: `--output-diff="$UNSET"` asked for a read-only diff and `--check-file="$UNSET"` for one file, and treating either as unset would sort and rewrite the whole repository instead.
- Keep an escaped comma inside an option value. `\,` is part of the value — `$permissions=sync-xhr=()\,camera=()` is one option — but the option list was split on every comma, so sorting reordered the halves into `$camera=(),permissions=sync-xhr=()\`, leaving a dangling backslash and a broken rule. This happened on every sort, in 5.5.0 as well.
- Add `--threads=N`, and a `threads` line in `.fopconfig`, for the worker pool. It was reachable only through `RAYON_NUM_THREADS`, which is rayon's variable rather than fop's and cannot be set for one invocation without exporting it. `--threads` takes precedence over the variable, and either overrides the default cap: 8 is a default chosen for the workload fop is usually pointed at, not a limit on what may be asked for. A value below 1 or not a number is rejected with exit 2; an absurd one is held to 1024, so a typo cannot try to spawn a thread per digit. `--show-config` now reports the pool that will actually be built and where the number came from, rather than re-deriving it — it used to print `auto (capped at 8)` while `RAYON_NUM_THREADS=16` was in force — and credits the variable only when its value parsed, so a malformed one reads as `auto` instead of sending someone to inspect an environment that had no effect. Measured on 32 cores over the EasyList repo (159 files, 197k lines): 1 thread 0.180s/39 MB, 4 0.062s/89 MB, 8 0.046s/126 MB, 16 0.039s/202 MB, 32 0.039s/300 MB. Output is identical at every count.
- Stop `filter_tidy` allocating to test for a byte. Deciding whether a rule carries an option whose value may hold spaces built `"$name"` and `",name"` for each of eleven names and searched for them — twenty-two `String` allocations per network rule. It now scans for the name and checks the byte before it, and since the answer only matters when the rule holds whitespace, which almost none do, the cheap test gates the expensive one instead of running after it. Around 2% on a 59,812-rule network-only corpus, with output byte-identical across 327 corpus files.
- Keep AdGuard's noop modifier intact. `$_____` is a run of underscores carrying no meaning, used to keep a long rule readable. `filter_tidy` normalises `_` to `-` in an option name for the sake of `redirect_rule`, and applied to a name that is nothing but underscores it produced `-----`, which is not an option at all — all 27 occurrences in AdguardFilters were rewritten that way. `is_known_option` did not recognise it either, so the addition checks called it an unknown option, which is a defect and so one `--remove-bad-rules` deletes. The corpus escaped that only because those rules are long enough that the option list fails to parse and the check is skipped; a short one such as `||example.com^$script,__,domain=site.example` was flagged. It is now recognised by shape, since the run can be any length.
- Treat `:contains()` as extended syntax, as `:has-text()` already was. The two are the same construct under AdGuard and ABP's respective names, but only one was on the list, so `:contains()` arguments went through selector tidying. Three AdGuard rules were being broken outright, because `+` in the argument was read as a sibling combinator and padded: `:contains(/^\u00A0+$/)` became `:contains(/^\u00A0 + $/)` and `:contains(/^ad\s+$/)` became `:contains(/^ad\s + $/)`, turning a "one or more" quantifier into a literal space-plus-space, and `:contains(Реклама 18+.)` had its literal text changed. The cost is that selector whitespace is no longer normalised in the 3,003 AdGuard rules using it — three lines in that corpus keep spacing fop would have tidied — which is the right side to err on when the alternative is rewriting an author's regex.
- Do not lowercase a `:` that is not a pseudo-class. `PSEUDO_PATTERN` matches any colon followed by letters holding an uppercase, which caught two shapes that must keep their case. An escaped colon is a literal one in an id or class name, both case-sensitive: AdGuard's `###js\:cookies\:barInitWrapper` became `barinitwrapper`, and the Tailwind class in `##...lg\:max-w-homepageContent` became `homepagecontent`. A colon opening a regex group — `(?:`, `(?i:`, `(?-is:` — is regex syntax: `:contains(/^(?:Reklama$|...)/)` became `/^(?:reklama$|...)/`, silently changing which text the rule matched. The backslash run is counted rather than merely tested, because an even run escapes the backslash and leaves the colon live, so `div\\:HOVER` really is a pseudo-class and is still lowercased. The replacement is applied by byte range rather than `replacen`, so a name occurring more than once is lowercased where it was found instead of at its first occurrence. `UNICODE_SELECTOR` is tested only once a candidate has survived the guards, rather than on every element rule.
- Keep the spaces in an `$addheader=` value, which `$header=` did not cover: that match is on `$header=`/`,header=`, so `$addheader=response:set-cookie:x=c; path=/; max-age=21600` was rewritten into `...x=c;path=/;max-age=21600`.
- Do not flag an unanchored hostname that carries options. Writing `$csp=` or `$redirect-rule=` is not done by accident, and the author who wrote one chose the matching as well. ABP's anti-circumvention list publishes 13 such rules (`billboard.com^$csp=script-src-attr 'none'` and friends) and uAssets another in `host-cdn.net^$image,redirect-rule=32x32.png,...`; all were flagged, and since `--remove-bad-rules` deletes advice along with defects, all were deleted. Mash is still flagged with or without options — a dotless, vowel-less token is nobody's deliberate choice — and the bare forms the check exists for, `example.com^` and `exa mple.com^`, carry no options and are caught where the whole line is the pattern.
- Do not read an ABP snippet as a CSS selector. `#$#` is two unrelated things: AdGuard injects CSS with it, which is selector-shaped and worth balancing, while ABP invokes a snippet, whose arguments hold regex literals and quoted strings where a bracket is data rather than syntax. Four snippets in the anti-circumvention list read as "unbalanced brackets in selector" and were deleted — a `\(` inside a regex, `[^>]` inside an XPath. The two are told apart by the opening: a CSS injection opens on a selector and carries a ` {` block, a snippet opens on its name. Testing the opening rather than a trailing `}` keeps a truncated injection catchable, and testing ` {` rather than any `{` keeps a snippet whose argument holds one — pluto.tv's regex quantifier `{1,2}` — from reading as CSS.

- Never commit over an unresolved merge. The pull fop runs before committing uses `--autostash`, and `git pull --rebase --autostash` exits 0 when popping the stash conflicts — it reports "Applying autostash resulted in conflicts" and leaves markers in the files. The result was discarded, so `commit -a` swept those markers into the commit and pushed them, printing `Commit successful` over the top. fop now checks for unmerged paths after that pull, and after the rebase in the push-retry path, and stops with instructions instead — exiting non-zero, so a script driving fop can tell a refusal from a clean run.
- Stop `--localhost` mode mangling hosts entries. The space between IP and host is the syntax, and `filter_tidy` strips whitespace from anything that is not an element rule — so `0.0.0.0 keep.com` was written back as `0.0.0.0keep.com`, breaking every hosts file fop sorted in that mode, 5.5.0 included. Such a line is now passed through as written. With `--remove-bad-rules` the consequence was worse: the mangled line reads as a bare domain, so the checks flagged it and the whole file was emptied.
- Keep the spaces in a `$header=`, `$responseheader=`, `$requestheader=` or `$permissions=` value. `filter_tidy` strips whitespace from network rules, which is right for `|| x .com ^` and wrong for `content-type:text/html; charset=utf-8` — the rule was silently rewritten into something that matches differently. `$csp=` and the others already on that list are unaffected.
- Do not suggest an option as the correction for itself. A bare `$requestheader` is wrong because it needs a value, and "did you mean requestheader?" says nothing.
- Recognise `$requestheader=`. It is live in uAssets `filters-2026.txt` and an unknown option counts as a defect, so the rule was one `--remove-bad-rules` would delete and `--ci` would fail on.
- Treat a space in a pattern as advice rather than a defect. `filter_tidy` already strips such spaces on the sorting pass, so fop repairs the rule losslessly — failing CI over it, or deleting it, was wrong. The wording no longer claims the rule cannot match: ABP normalises spaces out of network filters, so it does match there.
- Require an anchor to be followed by rule text, and never judge a regex filter on its spaces. `@@ -3,6 +3,9 @@` in a patch, `| Option | Description |` in a table, and `@@/^https?:\/\/[^ ]+\/ads\//$script` were all read as rules with a space in the pattern.
- Flag a space in the pattern of a standard adblock rule (`||exa mple.com^`, `@@||exa mple.com^`, `exa mple.com^`). Across 609k lines of EasyList and the region lists not one rule has a space there, so such a rule can never match. Spaces elsewhere are left alone: all 41 rules in those lists with a space in an option value are `$csp=` directives, and cosmetic selectors, hosts entries and comments carry them routinely. Limited to lines that open with `||`, `|` or `@@`, or end in `^` — a `^` mid-string is a regex anchor in someone's shell, not a separator — and only where the pattern half is known: a rule whose option list does not parse, such as a uBO `$replace=` carrying HTML, gives no way to say where the pattern ends.
- Treat `+++` as a file header only before a hunk begins. A rule reading `++ b/other.txt` arrives in the diff as `+++ b/other.txt`, and taking it as a header dropped the rule, shifted every later line number, and repointed the parser at a file the commit never touched — which `--remove-bad-rules` would then have gone looking in.
- Report nothing from a combined diff (`diff --cc`, emitted while a merge is unresolved) rather than wrong lines: its two status columns and `@@@` hunk headers are not parsed here.
- Share the diff reader with the `--ci` banned-domain audit instead of keeping a second copy. That copy had drifted: no `--no-color`, so a colourised diff made the audit pass having read nothing; `+++` read as a header, so a rule beginning with `+` escaped the check; and no handling for a quoted non-ASCII path.
- Fix a rule beginning with `+` being dropped from the diff and shifting every finding after it. Such a rule reaches `git diff` as `+++...`, which the parser mistook for a `+++ b/file` header — so it was never checked, and because it also left the line counter behind, later findings named a line one too low and the content check that guards removal rejected all of them (`the line no longer matches what was flagged`, `Removed 0 line(s)`).
- Recognise a rule's option list by scanning rather than with `OPTION_PATTERN`. The regex leads with `.*` and cost around 650ns on a rule carrying options, against 15ns for the byte paths — roughly half the cost of checking an added line, for one regex. Behaviour is unchanged — the scan is held to producing the same split as the regex, and agrees on all 609k lines of EasyList and the region lists as well as on the awkward shapes those do not contain. 50ns per rule overall, down from 92ns.
- Flag an unanchored pattern that reads as nothing at all (`fdfdgfgdgfd^`, `fdgfgdfgd`, `fdgfgdfgd$third-party`), with or without a trailing `^` and past any leading `-`, `+` or `_`: six or more letters, no digits or punctuation, and not one vowel. Those leading characters are boundary markers rather than syntax, so mash could otherwise escape by wearing one; they are stepped over for this test only, since `-ad.com^` keeps its boundary deliberately. It matches that text anywhere in a URL. The bar is deliberately narrow — the form as a whole appears nowhere in 609k lines of EasyList and the region lists, but that only means it is unused, and `doubleclick^`, `prebid^` or `300x250^` are patterns someone could reasonably write. Reported in its own words rather than as a missing `||`, since there is no host to suggest anchoring.
- Say which thing is missing when the rule checks cannot run. fop looks for `.git` in the directory it is given, so running it from a subdirectory found no repository — but the warning blamed git, which was working fine.
- Run the addition checks before the timestamp and checksum passes. Those hash the file body, so removing a line afterwards left the checksum describing content that was no longer there — and with the removal now followed by a commit, that invalid checksum would have been published.
- Leave the flagged lines in place under `--output`, `--output-diff` and `--benchmark`: those ask for a report, and the sorter already writes nothing in that mode. The run still stops at the prompt, so a dry run cannot commit the rules it declined to remove.
- Make `remove-bad-rules` in `.fopconfig` imply `check-rules-on-add`, as the command-line flag already did. On its own it left the checks off and so did nothing at all.
- Restrict the checks to the files `--ignore-all-but` selects, so a run told to touch one file cannot rewrite every list in the repository's diff.
- Treat a diff that cannot be read as a failure rather than as "nothing was added", both before and after removing lines. The earlier code turned it into an empty list, which would have reported a clean bill of health for a check that never ran and could have committed the flagged lines.
- Run the addition checks in sort-only mode (`--no-commit`, `--just-sort`) as well. They were inside the commit flow, so the flags were accepted and silently did nothing; repository detection, which the commit flow also gated, now happens whenever the checks are on. Without a commit to gate there is no prompt — findings are reported, and `--remove-bad-rules` still removes them.
- Flag a host rule that lost its `||` anchor (`rbush.shop^`, with or without options). Only when the pattern carries a `^`: a hostname with options and no separator is a substring pattern, and whole files are written that way — `easyprivacy_general_emailtrackers.txt` holds 319 unanchored rules and not one anchored one. It is legal and matches the name *anywhere* in a URL, so it also blocks `lampedburbush.shop` and anything else ending in it — over-blocking that stays invisible until a site is reported broken. Reported as advice, never deleted, since the fix is to add the anchor. Deliberate uses barely exist: across 609k lines of EasyList and the region lists there was one, itself a typo (`arketing.indianadunes.com^`, missing its leading `m`).
- Merge a `:has-text()` group carrying regex flags, keeping them: `/a/i` with `/b/i`, or with plain `b`, becomes `/a|b/i`. One flagged argument sets the flags for the whole group, which does widen the plain text — `Sponsored` beside `/…/i` becomes case-insensitive — and is what writing `/i` next to it means. Two different flag sets have no single form to merge into, so `/a/i` with `/b/m` is left alone, as is an empty alternative (`/foo|/`, which matches everything, so dropping it narrows the rule). A lone `/` is no longer treated as a regex either — slicing it panicked and aborted the whole sort.
- Merge `:has-text()` rules for `##` only. Everything else is left alone, and the scan stops at whichever separator comes first rather than stepping over one it cannot merge — which previously let `#@##ad` split at the `##` those two `#` characters form and merge two exceptions. An exception (`#@#`, `#@?#`) cancels a hiding rule by matching its selector *text*, so folding two of them would leave neither original string in existence and the rules they cancelled would no longer be excepted; `#$#`/`#%#` inject CSS and JavaScript, where `:has-text()` means nothing; and `#?#` is excluded because merging rewrites `:-abp-contains(text)` into `:-abp-contains(/regex/)`, which assumes whatever reads that separator accepts a regex there.
- Deduplicate the alternatives when merging, so a part-merged group does not grow. `/A|B/` plus `A` plus `B` gave `/A|B|A|B/`, and each later run compounded it; splitting is top-level only, so `(a|b)c` stays one alternative.
- Do not merge a nested `:has(span:has-text(x))`. The pattern matches lazily, leaving the base with an unclosed `(` and the argument with an extra `)`, so the rebuilt rule was a bracket short and its regex searched for a literal `)`. Such groups are now left alone.
- Add `--check-rules-on-add` (`.fopconfig`: `check-rules-on-add`) to check newly added lines for rules that cannot work, reported with file and line and prompting before the commit goes ahead. It catches a separator with no selector (`example.com##`), a truncated selector (`##.ad[href="x"`), an option marker with no options (`||example.com$`), an option with no value (`$domain=`) and an unrecognised option (`$thrid-party`). AdGuard HTML-filtering rules (`example.com$$amp-consent`, and the `$@$` exception) are judged as the cosmetic rules they are, split at whichever separator comes first, rather than having their tag read as an option list. An unrecognised option is matched against the known set by edit distance and reported with a suggestion — `unknown option: thrid-party -- did you mean third-party?` — rather than against a table of known misspellings, so a typo nobody has seen before is still named. An option that is simply new gets no suggestion instead of a wrong one. `--remove-bad-rules` deletes the defective lines instead of prompting and then commits what is left, so a run that adds two good rules and one bad one lands the two. Advice — a bare hostname, an unanchored host — is kept and reported, never deleted: it is legal syntax, and in a plain domain-list file it is exactly what belongs there. `--ci` likewise fails only on defects. The checks read the diff against `HEAD`, which is what `commit -a` takes, so a rule already `git add`ed is checked along with unstaged ones. They run before sorting, so they judge — and `--remove-bad-rules` deletes — only lines the author wrote: run after, they saw the sort's merges, and an added `b..com##.ad` beside a committed `a.com##.ad` became `a.com,b..com##.ad`, which was flagged and deleted along with the committed rule. Each line is still judged in the form the sort will write it, so `$redirect_rule=`, `$Third-Party` or `$SCRIPT` — unknown options as typed, repaired by the sort — are fixed rather than deleted. A line that may already hold a committed rule, because the file was sorted since the last commit, is reported and left for a manual fix rather than deleted.
- Flag a bare hostname written as a rule (`domain.com`, `anotherdomain.co.nz`) and suggest `||host^`. It is legal syntax — it matches the name anywhere in a URL — but it also matches `notdomain.com.evil.test` and any URL merely mentioning the name, and it is almost never what was meant. Reported as advice, never deleted by `--remove-bad-rules`: in a plain domain-list file a bare host is exactly what belongs there. Across 608k lines of EasyList and the region lists every instance was in such a file, and no genuine filter list carried one.
- The same pass also flags a malformed domain list (`,example.com##`, `a.com,,b.com##`, `exa..mple.com##`), a selector opening on a combinator (`##> div`), and an empty entry in a `|`-separated option value (`$domain=a.com|`). Balancing covers `{}` as well as `[]` and `()`, so AdGuard CSS injection is checked too.
- Run the same checks in `--ci`: with `--check-rules-on-add`, a CI run audits what the branch added since it forked from the default branch (or the last commit, when HEAD already matches the default branch or none can be found) and exits 1 on a defect. Advice such as a bare hostname is printed as a `Notice:` and does not fail the build. `ignorefiles` is honoured. Without the flag, `--ci` behaves exactly as before.
- Restrict the addition checks to files fop would sort. They ran on every added line in the repository, so a `$` in a shell script or a workflow (`export PATH=$PATH:/usr/bin`, `run: echo "$GITHUB_SHA"`) read as a network rule with an unknown option — deleted by `--remove-bad-rules`, and a CI failure for a pull request that touched no filter list at all.
- Recognise a rule's options with the same pattern the sorter uses, rather than by taking the last `$` and splitting on every comma. `$removeparam=/^utm$/`, `$replace=/(foo)bar/$1baz/` and comma-bearing `jsonprune=`/`xmlprune=` values were all reported as defects.
- Exempt AdGuard JavaScript injection (`#%#`, `#@%#`) and scriptlet bodies from the selector checks: they are not CSS, so an apostrophe in a comment is not an unbalanced quote.
- Add `inline-font`, `beacon`, `mp4`, `noop`, `queryprune`, `strict-first-party`, `strict-third-party` and `webbundle` to the known options, along with AdGuard's DNS modifiers (`dnsrewrite=`, `dnstype=`, `client=`, `ctag=`) and the modifiers that are valid bare on an exception rule, where they switch a whole class off: `@@||site^$urlblock`, `$removeheader`, `$replace`, `$redirect`, `$permissions`, `$urltransform` and their kin. Those bare forms stay unknown on a blocking rule, where they are missing a value. The omission was harmless while an unknown option was only a warning; with the addition checks it deleted valid rules.
- Resolve the CI base from the remote's default branch instead of assuming `origin/master`, and diff from the fork point rather than the branch tip, so a branch that is behind does not see the default branch's deletions as its own additions. With no default branch to find — a shallow PR checkout fetches only the merge ref — the last commit is used; the audit fails only when neither resolves. That includes the default `actions/checkout`, `fetch-depth: 1`, which holds no history to compare with: `--ci` used to pass there having checked nothing, and now fails, naming `fetch-depth: 2` as the fix. On a `main` repository, or a shallow clone missing `HEAD~1`, the diff used to fail silently and the audit passed having checked nothing.
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
