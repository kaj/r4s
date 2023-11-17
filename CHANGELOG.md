# Changelog for r4s

Notable changes may be documented in this file.
The format is (loosely) based on
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this
project more or less adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

* Refactored asset handling in server to a separate module.
* Updated to `diesel-async` 0.4.1, `lazy-regex` 3.0.0, and
  `ructe` 0.17.0.


## Release 0.3.4
2023-07-02 15:40 CST

* Implemented graceful shutdown.
* Improved how common headers are added to responses.
* Updated to `disel` 2.1.0 and `diesel-async` 0.3.1.
* Updated to `rust_icu_ucol` 4.0.0, `qr_code` 2.0.0, and
  `accept-language` 3.0.1.


## Release 0.3.2
2023-02-28 20:20 CST

* Clean up diesel usage (PR #8).
  - Use `belonging_to` and (rust-side) `grouped_by` to replace 1+n
    queryes with 1+1 for loading tags on pages.
  - the `post_tags` relation table does not need a separate `id` column.
  - Use `Model::as_select()` instead of column tuples some places.
  - Don't select the year for a post separately when getting the `posted_at`.
* Limit the db pool to 20 connections (default is too large for my server).
* Make the decoration of the comment hr visible again.


## Release 0.3.0
2023-01-24 19:18 CST.

* Bugfix: Handle bilingual drafts (don't remove both before updating one).
* Style update: Use oldstyle numbers in text and only show the top
  bike if there s room for it.
* Style addition: Add some illumination to initials (PR #7).
* Added common headers (including basic CSP).
* Addded mastodon link to `#me_box`.
* Updated diesel to 2.0 and use diesel-async.  The db code in async
  views becomes simpler by not needing the `interact` wrapper, but on
  the other hand all db access now needs a `&mut db` (PR #6).
* Updated to `atom_syndication` 0.12.0, `base64` 0.21.0, `clap` 4.0.18,
 `ructe` 0.16.0, `rust_icu_ucol` 3.0.


## Release 0.2.18
2022-08-01 17:24 CEST.

* Add fallback route for post urls without language.
* Improve print styling.
* Use textwrap to improve moderate-comments formatting.
* Use clap 3.2.5 instead of structopt.


## Release 0.2.16
2022-05-16 23:18 CEST.

* Update syntect (the syntax highlighter) to 5.0.0.
* Implement a theme switcher UI (PR #5)
* Add Secure flag to the CSRF cookie.


## Release 0.2.14
2022-04-05 19:05 CEST.

* Improve webkit/chrome compatibility in the stylesheet.


## Release 0.2.12
2022-04-03 18:26 CEST.

* Improve how youtube videos are handled (PR #4).
* Specify that the main font should be serif.
* Some refactoring.


## Release 0.2.10
2022-03-01 22:20 CET.

* Support `!embed` blocks for youtube, in preparation for improving
  privacy by not contacting youtube before the user starts a video.
* Bring back my `robots.txt`.
* Updated ructe to 0.14.0, improving rendering error handling.
* Improved error handling, getting rid of some `unwrap()` calls.


## Release 0.2.8
2022-02-01 23:52 CET.

* Bugfix: Fallback route must be last, so it don't hide the feed routes.


## Release 0.2.6
2022-02-01 20:37 CET.

* Support qr-code blocks.  Text from the block is made into a qr-code,
  encoded as a png image in a data: url.
* read-comments is no longer async (there was no await in it).
* read-files now handles keyword links (e.g. [term][wp]) correctly
  when the term is written across a line-break.
* Update content license to cc-by 4.0.


## Release 0.2.4
2022-01-23 00:20 CET.

* Don't hardcode img.krats.se, I use local image server for drafting.
* Put a div around .gallery images, and use flex layout for them.
* Special handling of `[x]`, assumed to designate a keyboard key.


## Release 0.2.2
2022-01-20 18:30 CET.

* Comment poster url validation, avoid empty non-null urls.
  Also, actually link to the url when given.  And log some more.
* Add --silent --list arguments to moderate-comments, for background check.
* Proper parameter handling for twitter and fb urls.
* Try to label the sections that are comments.
* Implemented fallback for some language-less urls.


## Release 0.2.0

Initial release, Sunday 2022-01-16 22:35 CET.
There is posts, meta pages and comments.
There is syntax highlighting in code samples, and rphotos support for
images.

Initial development started Sunday 2021-10-17.
