# Debug: macOS sign-in says "you are not logged in"

Handoff for an agent on the macOS 27 work laptop. Delete this file once the
issue is fixed.

## Symptom

On branch `scan-browser-profiles`, the sign-in scan finds the right Chrome
profile, but clicking **Use** fails with:

    Invalid cookies: auth error: you are not logged in

The user *is* signed in, and was playing music on music.youtube.com in that
profile.

## What we know

- The error is rustypipe's `AuthError::NoLogin`, raised in
  `extract_session_headers` (`rustypipe-0.11.4/src/client/mod.rs`, around
  line 1691). It GETs `https://www.youtube.com/results?search_query=` with the
  cookies, and fails when the HTML has no `"USER_SESSION_ID"`. That means
  YouTube did not accept the cookies as a signed-in session.
- rustypipe only reads cookies whose domain is exactly `.youtube.com`
  (`util::parse_netscape_cookies(cookies, ".youtube.com")`).
- Commit `bf9b58f` makes the scan count a profile as signed in only when it
  has `SAPISID` / `__Secure-*PAPISID` on `.youtube.com`. Before, a login
  cookie on `.google.com` was enough. Re-test with this commit first.
- yt-dlp without a profile takes the most recently modified profile's
  `Cookies` DB. The app now always passes the absolute profile dir
  (`chrome:<path>`), so this is not the cause any more.
- The laptop's yt-dlp is newer than 2026.08.19 (the version on the Linux
  machine this was written on).

## Steps

Never print cookie values. Names, counts and yt-dlp's own messages only.

0. Rebuild and re-test: `git pull && cargo run --release`, open Sign in, and
   try the profile again. If it is no longer listed as signed in, that is
   case A below.

1. Find the profile dir. The profile folder names and their display names
   are in `~/Library/Application Support/Google/Chrome/Local State`, under
   `profile.info_cache`:

   ```sh
   python3 -c "import json,os;d=json.load(open(os.path.expanduser('~/Library/Application Support/Google/Chrome/Local State')));[print(k,'=>',v.get('name'),v.get('user_name')) for k,v in d['profile']['info_cache'].items()]"
   ```

2. Export that profile's cookies and check them. Replace `Profile 1` with
   the folder from step 1.

   ```sh
   P="$HOME/Library/Application Support/Google/Chrome/Profile 1"
   yt-dlp --version
   yt-dlp -v --cookies-from-browser "chrome:$P" --cookies /tmp/c.txt --skip-download https://music.youtube.com 2>&1 | grep -E "Extracting cookies from:|Extracted|decrypt|version breakdown|WARNING"
   awk -F'\t' '{sub(/^#HttpOnly_/,"",$1)} $1==".youtube.com"{print $6}' /tmp/c.txt | sort | tr '\n' ' '; echo
   curl -s -b /tmp/c.txt 'https://www.youtube.com/results?search_query=' | grep -c '"USER_SESSION_ID"'
   rm /tmp/c.txt
   ```

## Reading the result

- **A. No `SAPISID` / `__Secure-1PAPISID` / `__Secure-3PAPISID` among the
  `.youtube.com` names.** The profile's YouTube login is not on
  `.youtube.com`. Check whether the session lives on another domain:
  `awk -F'\t' '{print $1}' /tmp/c.txt | sort | uniq -c` before the `rm`.
- **B. "N could not be decrypted", with N above 0, or a version breakdown
  with unexpected versions.** Chrome or macOS 27 changed cookie encryption,
  and yt-dlp only reads part of the jar. Check yt-dlp's issue tracker for
  the macOS 27 / current Chrome version. The fix is likely upstream in
  yt-dlp.
- **C. Login names present, nothing failed to decrypt, but curl prints
  `0`.** YouTube rejects the copied cookies. Unverified suspect: Chrome's
  device-bound session credentials (DBSC), which bind Google session cookies
  to the device, so copies stop working.
  - To confirm: export the same account from Firefox or Safari and repeat
    the curl check. If that one prints `1`, it's Chrome-specific.
  - The fix would then be a different sign-in route, not cookie code.
- **D. curl prints `1` but the app still fails.** The problem is on the app
  side. Diff the exported jar against what `filter_cookies` keeps
  (`src/auth.rs`); compare names only. Check that `.youtube.com` login
  cookies survive filtering.

## Where the code is

- Profile discovery, spec builder, cookie filters: `src/auth.rs`
  (`scan_profiles`, `browser_spec`, `has_login_cookie`, `filter_cookies`).
- yt-dlp import and scan: `src/backend.rs` (`import_profile`,
  `scan_browsers`, the `Cmd::SignIn` arm).
- Dialog: `src/ui/library.rs` (`draw_sign_in_dialog`).
- Tests: `cargo test`.
