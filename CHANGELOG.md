# Changelog for r4s

Notable changes may be documented in this file.
The format is (loosely) based on
[Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this
project more or less adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

* Add fallback route for post urls without language.
* Fix incosistency in moderate-comments formatting.
* Use clap 3.2.5 instead of structopt.


## Release 0.2.16

* Update syntect (the syntax highlighter) to 5.0.0.
* Implement a theme switcher UI (PR #5)
* Add Secure flag to the CSRF cookie.


## Release 0.2.14

* Improve webkit/chrome compatibility in the stylesheet.


## Release 0.2.12

* Improve how youtube videos are handled (PR #4).
* Specify that the main font should be serif.
* Some refactoring.


## Release 0.2.10

2022-03-01 22:20 CET

* Support `!embed` blocks for youtube, in preparation for improving
  privacy by not contacting youtube before the user starts a video.
* Bring back my `robots.txt`.
* Updated ructe to 0.14.0, improving rendering error handling.
* Improved error handling, getting rid of some `unwrap()` calls.


## Release 0.2.8

2022-02-01 23:52 CET

* Bugfix: Fallback route must be last, so it don't hide the feed routes.


## Release 0.2.6

2022-02-01 20:37 CET

* Support qr-code blocks.  Text from the block is made into a qr-code,
  encoded as a png image in a data: url.
* read-comments is no longer async (there was no await in it).
* read-files now handles keyword links (e.g. [term][wp]) correctly
  when the term is written across a line-break.
* Update content license to cc-by 4.0.


## Release 0.2.4

* Don't hardcode img.krats.se, I use local image server for drafting.
* Put a div around .gallery images, and use flex layout for them.
* Special handling of `[x]`, assumed to designate a keyboard key.


## Release 0.2.2

* Comment poster url validation, avoid empty non-null urls.
  Also, actually link to the url when given.  And log some more.
* Add --silent --list arguments to moderate-comments, for background check.
* Proper parameter handling for twitter and fb urls.
* Try to label the sections that are comments.
* Implemented fallback for some language-less urls.


## Release 0.2.0

Initial release, Sunday 2022-01-16.
There is posts, meta pages and comments.
There is syntax highlighting in code samples, and rphotos support for
images.

Initial development started Sunday 2021-10-17.
