# Changelog

All notable changes to RustDirStat, newest first.

**This file is generated.** It is rebuilt from the git history by
`cargo run --example changelog`, so released sections are rewritten in
place and edits to them do not survive. See `CONTRIBUTING.md`.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Internal

- **deps:** bump the github-actions group with 4 updates ([`7a0d8c0`](https://github.com/lnorton89/rustdirstat/commit/7a0d8c063f9884cfe504277fd09923026d5ef62f))

## [0.2.1] - 2026-08-21

### Added

- finish the 0.2.0 review — every remaining finding implemented ([`924b1c4`](https://github.com/lnorton89/rustdirstat/commit/924b1c4c07b2e04a9247f82fc4b00685c44dcd75))

### Fixed

- skip the non-UTF-8 name tests where the filesystem forbids the name ([`82749b8`](https://github.com/lnorton89/rustdirstat/commit/82749b8e326109103f0f54f0868756def3cbc3c4))
- address the second branch review end to end ([`8f278c8`](https://github.com/lnorton89/rustdirstat/commit/8f278c8d4dc069eec87c088c2858f00925e48a52))
- address the branch review — release artifacts, exact selection, async zoom extensions, deny CI ([`ec4a98a`](https://github.com/lnorton89/rustdirstat/commit/ec4a98a168b34ed41a9e49784a417537be1d5b3e))
- exact tree lookups by default, and real physical size on Windows ([`0044619`](https://github.com/lnorton89/rustdirstat/commit/00446190fb3bfc66a72b80fa463cc196796ed935))
- report duplicate read failures, flatten rows iteratively, atomic config ([`092f989`](https://github.com/lnorton89/rustdirstat/commit/092f989f83ca6f090a2aea4821bd3fa90e565ea1))
- vssadmin invocation, CLI mode conflicts, and pinned actions ([`622c7b3`](https://github.com/lnorton89/rustdirstat/commit/622c7b39d07a197530a510ab823990463028e8b7))
- hard links, filesystem boundaries, and background GUI search ([`0b12dd0`](https://github.com/lnorton89/rustdirstat/commit/0b12dd0413db2fb62d5b8f87cbfa87c46a19e241))
- harden destructive operations — moves, empties, and rescan restore ([`0c734f8`](https://github.com/lnorton89/rustdirstat/commit/0c734f848ad862fcc54bc9a927fb919c1dd7104b))
- keep filesystem identity lossless — Node.name is OsString ([`5921f7d`](https://github.com/lnorton89/rustdirstat/commit/5921f7d16fe344a7c4a5a0f05d77594c1738aa15))

### Changed

- one home per constant, and every GUI spacing on the scale ([`5f97375`](https://github.com/lnorton89/rustdirstat/commit/5f97375e9bb909d9d221e382bb4e10d0b0030f1b))
- dedupe the pre-order walk and test scratch dirs; doc the new invariants ([`c6e076b`](https://github.com/lnorton89/rustdirstat/commit/c6e076b97616577a9458a600d34dcf2705a60b10))

### Documentation

- record the constants, changelog, and dependency conventions ([`9c8415c`](https://github.com/lnorton89/rustdirstat/commit/9c8415ce93234097bbf97bd44dc50c6879107c69))
- add contribution guides, issue/PR templates, and CODEOWNERS ([`27ba022`](https://github.com/lnorton89/rustdirstat/commit/27ba022077a9fa1f5a201aa332f37723b4a833e8))

### Internal

- bump version to 0.2.1 ([`3d07710`](https://github.com/lnorton89/rustdirstat/commit/3d07710c0a975d872059a83927e0573eb4b00b69))
- generate CHANGELOG.md from the git history ([`4652cc2`](https://github.com/lnorton89/rustdirstat/commit/4652cc2c6b0c940123353b943fce2d0c3573be19))
- group Dependabot by patch only, and freeze the egui stack ([`c457327`](https://github.com/lnorton89/rustdirstat/commit/c457327f85e466bebfa0555be7f5436a2dde1164))
- pin the Nix dev shell to rust-toolchain.toml via fenix ([`38031eb`](https://github.com/lnorton89/rustdirstat/commit/38031ebda8e3510e83085aae7b84b248d141a088))

## [0.2.0] - 2026-08-21

### Added

- give the app one brand mark, and the README the art from it ([`3ad4a34`](https://github.com/lnorton89/rustdirstat/commit/3ad4a34cc9a0af30683f947c1af3d600069cf65a))
- support character classes and alternation in search globs ([`028904e`](https://github.com/lnorton89/rustdirstat/commit/028904e29ecf3196fbacb882b966e9e9f0b76a48))
- native file-type icons, snappier scans, ext column resizing ([`86edf72`](https://github.com/lnorton89/rustdirstat/commit/86edf729cf99b33010ff8a6a2503e6eae88e4f21))
- file categories section, type icons, and working column resizing ([`a4fd371`](https://github.com/lnorton89/rustdirstat/commit/a4fd3715ce4921c4a9a34792e7f727456b0584cb))
- add a Nix flake ([`daf1682`](https://github.com/lnorton89/rustdirstat/commit/daf168250ab5b08b86e7cfbe439b975615280cf3))

### Fixed

- no console window behind the GUI, and an icon on the executable ([`84ffd15`](https://github.com/lnorton89/rustdirstat/commit/84ffd1510838993e532ac527826d43901ce3b4a3))
- stop the file list dropping columns when the pane narrows ([`65c94be`](https://github.com/lnorton89/rustdirstat/commit/65c94be5c8c17e72a7b8af4a5cf0f6bde8443d0f))
- gate the row-cache comparison to debug builds ([`010a268`](https://github.com/lnorton89/rustdirstat/commit/010a2688ab038777f142e1dd0d60c35d5a4b1b01))
- stop the extensions pane pinning itself open, and let it scroll ([`4b7a04a`](https://github.com/lnorton89/rustdirstat/commit/4b7a04a2a4610ef0caa49a5d48003fff0de7d7d6))
- **ci:** install a literal toolchain version, not an empty env var ([`400ca6e`](https://github.com/lnorton89/rustdirstat/commit/400ca6eb20884fb051c35ea0de6e614c94d3c72d))
- build popup buttons and their click targets from one list ([`911919e`](https://github.com/lnorton89/rustdirstat/commit/911919e5f9d23998d061ed27b7e3d99ab74481ea))
- three gist findings I had marked done but had not verified ([`204258d`](https://github.com/lnorton89/rustdirstat/commit/204258d92efdfcaf5e27241c82fba454088321dd))
- let a table give width back when its pane shrinks ([`b10558e`](https://github.com/lnorton89/rustdirstat/commit/b10558e0ebd3e58fdc8b56c56562a761ca8d14fe))
- bound the scanner's recursion without giving up its parallelism ([`70400ae`](https://github.com/lnorton89/rustdirstat/commit/70400aeaccfa0ff677c0084c0f292016b62dba3e))
- copy directory trees iteratively, and test that it copies them ([`a28f975`](https://github.com/lnorton89/rustdirstat/commit/a28f975701c5ad674a72fdf39206e356d5d1377f))
- correct clipboard encoding, and empty the Recycle Bin directly ([`d92133d`](https://github.com/lnorton89/rustdirstat/commit/d92133d6df7ac0ae143c9eef1f36a81fd0cb7c22))
- stream the CSV export, and free trees without recursing ([`6c3dde7`](https://github.com/lnorton89/rustdirstat/commit/6c3dde7e8b2bdb420cced168fcc4b36a1a3bbb6e))
- duplicate scan takes whole groups and says what it skipped ([`6c73353`](https://github.com/lnorton89/rustdirstat/commit/6c73353db3542aa73aef53ea418c2e53a86b6d43))
- one extension colour scheme for both front ends ([`4237497`](https://github.com/lnorton89/rustdirstat/commit/4237497635c82443ddf1634999adac6797b289a3))
- stop stray keys cancelling destructive confirmations ([`60b1be0`](https://github.com/lnorton89/rustdirstat/commit/60b1be04129a841443bb70d3bb1903f7d0e6d65b))
- make every table column resizable, first one included ([`67b63f5`](https://github.com/lnorton89/rustdirstat/commit/67b63f5e116658da0fbaa3430b31136522a6e5b6))
- drop the gratuitous unsafe and give the screen DC an owner ([`86cd55c`](https://github.com/lnorton89/rustdirstat/commit/86cd55c0bb937c8eebf0e8263f52ccf3fa5b546c))
- tooltips, drag lag, end-of-scan freeze, duplicates scrollbar ([`62a73ad`](https://github.com/lnorton89/rustdirstat/commit/62a73ad0c38be602988bd50ffe9154d89ad1b741))
- pin the table sizing behaviours with a test that can catch them ([`6e1446a`](https://github.com/lnorton89/rustdirstat/commit/6e1446ad61fa6c3a4831ebacb8ff9ec7e5c3baf2))
- always-visible scrollbars, disabled tooltips, and column resizing ([`e759d43`](https://github.com/lnorton89/rustdirstat/commit/e759d430a74bb46b7b9eaaadf0715464a9b2edc8))
- reset the workspace on a new scan, and size labels from the font ([`9d4f15d`](https://github.com/lnorton89/rustdirstat/commit/9d4f15d0aa7e05a0def557e3246a4d3faf63b86a))
- menu bar spacing, a squeezed file list, and two treemap bugs ([`370b72e`](https://github.com/lnorton89/rustdirstat/commit/370b72e72e20d66e12c078bf48e5682bb239d338))
- draw the treemap by what fits, not by a fixed child count ([`ce6e253`](https://github.com/lnorton89/rustdirstat/commit/ce6e2537bb728fe6f332fa3048bc6e377f935687))
- draw the treemap by what fits, not by a fixed child count ([`07ea920`](https://github.com/lnorton89/rustdirstat/commit/07ea920789a233c1965bb589986f866090cb8fef))

### Changed

- talk to the clipboard directly instead of spawning a helper ([`ce90134`](https://github.com/lnorton89/rustdirstat/commit/ce90134dfd138176e36b18927145cd07d3cb72d7))
- move the view selector into the toolbar ([`ad3b1a4`](https://github.com/lnorton89/rustdirstat/commit/ad3b1a46b6a0fae891fb620a950d6edd09d50322))
- draw both flat file views from one table ([`c88c23a`](https://github.com/lnorton89/rustdirstat/commit/c88c23a857d4190417f91f634e575217878b7bb1))
- walk the tree once for a delete and an empty, not twice ([`3d4d1b1`](https://github.com/lnorton89/rustdirstat/commit/3d4d1b1f708e7bb97f0fcd51c4a96b4bf422de87))
- give the TUI the widgets module the GUI already had ([`4e77460`](https://github.com/lnorton89/rustdirstat/commit/4e77460dbdbeeea68bc07f0c32f52212c5d52da3))
- open a path through one launcher, not two copies of one ([`b3f29af`](https://github.com/lnorton89/rustdirstat/commit/b3f29af8622e26160a496d036587c94cfe15e93b))
- give both front ends one search, one top-files, and one sort ([`1d19b85`](https://github.com/lnorton89/rustdirstat/commit/1d19b858fd77e5a07cd49162752fbc8c16c7ea13))
- split the TUI app module by concern ([`e884342`](https://github.com/lnorton89/rustdirstat/commit/e884342fe93f474ab1ccdcc52b3584145d41b3ad))
- split the GUI app module by concern ([`a68cfa7`](https://github.com/lnorton89/rustdirstat/commit/a68cfa7680be9ef14901e34daa2f40a17967cd8f))
- group the GUI app's state, and pin the config round trip ([`c4f1dff`](https://github.com/lnorton89/rustdirstat/commit/c4f1dff705ba8e16f0f1db9e21784a779036a6eb))
- group the TUI app's state by the view that owns it ([`14441d8`](https://github.com/lnorton89/rustdirstat/commit/14441d84a09853ebacbeddc1effc8b63ca1219b6))
- split the TUI drawing module by screen region ([`b153dba`](https://github.com/lnorton89/rustdirstat/commit/b153dbac0a1bdfa30d9f20d3ccfc2ee0a8acf01a))

### Documentation

- record what the egui 0.36 migration actually involves ([`a8a83da`](https://github.com/lnorton89/rustdirstat/commit/a8a83daf09c1be35e6a0d8d22dc04a30a542292b))
- two rules I broke this session, written down ([`59d884f`](https://github.com/lnorton89/rustdirstat/commit/59d884f5f5be1f3101ea9ae2132bacf6438aaedf))
- record the egui layout rules this round cost time to find ([`4f9c639`](https://github.com/lnorton89/rustdirstat/commit/4f9c639701b2eb7a6cf58ffe72babb4ed9dfc4ea))
- record where shared code lives and what may not recurse ([`748a1b8`](https://github.com/lnorton89/rustdirstat/commit/748a1b8a613a30d41cbb2a61696f187a60e9fcd6))
- give every source file a header, and a test that enforces it ([`8e3cb0b`](https://github.com/lnorton89/rustdirstat/commit/8e3cb0b327171af4ffeac7db96ef5638a0d88d8a))
- record the conventions this round of fixes established ([`4e129af`](https://github.com/lnorton89/rustdirstat/commit/4e129afe9f81b644a8d0bd2450bee8535c129151))
- cover the Nix flake and cross-platform checking ([`9bf624f`](https://github.com/lnorton89/rustdirstat/commit/9bf624f99ed0e732e760f7b0dd4098d54e89c76a))

### Internal

- release v0.2.0 ([`a2fa793`](https://github.com/lnorton89/rustdirstat/commit/a2fa7939c0387266462dd92e3df92174a72cd0f1))
- ratatui 0.28 -> 0.30, crossterm 0.28 -> 0.29 ([`4a178ce`](https://github.com/lnorton89/rustdirstat/commit/4a178ce7c91e873ad2af0409613f34eb36af5fd1))
- one line-ending rule, enforced by git ([`2fe84ec`](https://github.com/lnorton89/rustdirstat/commit/2fe84ec817434c3fe1631153156010b6e1f1fd1a))
- pin the toolchain, and update what can be updated safely ([`c4d0b6c`](https://github.com/lnorton89/rustdirstat/commit/c4d0b6c3d388b71855650f8d3e1fa7958dffd8c4))
- build both macOS targets on the Apple Silicon runner ([`ca901ba`](https://github.com/lnorton89/rustdirstat/commit/ca901ba8e70be1b062c12a58a2ce3d56e8838996))

### Tests

- pin that the extension table scrolls rather than dropping columns ([`ccf5799`](https://github.com/lnorton89/rustdirstat/commit/ccf57993370dd5f15b3b003f0317bfd2b8e97fcb))
- pin the GUI's "a modal is modal for the keyboard too" rule ([`2c75339`](https://github.com/lnorton89/rustdirstat/commit/2c75339a2a5d7af0654729f885171ce83ea2c77b))
- cover the report's depth and truncation limits ([`3fe46e7`](https://github.com/lnorton89/rustdirstat/commit/3fe46e78bd0433c18a62610afcd722fd3b9084c5))
- cover the TUI treemap's traversal invariants ([`ed526f6`](https://github.com/lnorton89/rustdirstat/commit/ed526f63a6a6adafa1222c16850339fe2ba186f4))
- run the binary end to end over a fixture tree ([`fb67a28`](https://github.com/lnorton89/rustdirstat/commit/fb67a282ebbd03fd0865ff421af3c4fbdd65a093))
- cover the width-aware text helpers and the category stats ([`5615cf7`](https://github.com/lnorton89/rustdirstat/commit/5615cf722968d1501faf0ac04224851db0c078bd))
- cover the squarified treemap layout ([`db383fa`](https://github.com/lnorton89/rustdirstat/commit/db383fa1b98455166ccdd926521f3bd8fc7a727d))

### Other

- Revert the module move out of the previous commit ([`9b4858b`](https://github.com/lnorton89/rustdirstat/commit/9b4858b80ead88827a8ad299148e851273d1202a))
- Add documentation for flake usage ([`69ff1d5`](https://github.com/lnorton89/rustdirstat/commit/69ff1d519a75130e1bd8d3e56d87bbf730dbd22d))

## [0.1.0] - 2026-08-20

### Added

- add checkmark and bullet icons for menu state ([`10f9f6a`](https://github.com/lnorton89/rustdirstat/commit/10f9f6ae74cfea681644108815542e5273bd02f2))
- polish WinDirStat GUI parity and interactions ([`5be8f78`](https://github.com/lnorton89/rustdirstat/commit/5be8f7813fee064118ff004452d7164a3fb1c5c5))

### Fixed

- give every unsafe call a safe leaf wrapper, and drop a blanket allow ([`2229d05`](https://github.com/lnorton89/rustdirstat/commit/2229d05475258075ff6146fbbc92cffb7a48148d))
- two portability bugs CI surfaced on first run ([`04b4741`](https://github.com/lnorton89/rustdirstat/commit/04b474178cf69a761e32ca8d105a2c6ffe5b394f))
- keep the treemap covering the whole panel on large volumes ([`b01c383`](https://github.com/lnorton89/rustdirstat/commit/b01c38355cc3c4b51a53dfed86d1df6d9cf05b90))
- polish tables and treemap rendering ([`41fb19b`](https://github.com/lnorton89/rustdirstat/commit/41fb19b2f1b42cec083a03e7357a4ae6b16ccc41))
- match WinDirStat extension columns ([`a7f4c41`](https://github.com/lnorton89/rustdirstat/commit/a7f4c4110b98121ab7ef8c8937ef36d20b51635c))
- sort extension table and preserve row clicks ([`44c0eda`](https://github.com/lnorton89/rustdirstat/commit/44c0edae753bc13cf4724985d7c3d5bfd49a5727))
- show active column sort direction ([`355e042`](https://github.com/lnorton89/rustdirstat/commit/355e0428f992826dc953fb6ef66ed8edfe408472))

### Performance

- stop doing tree-sized work per frame, and split gui::ui up ([`55bc645`](https://github.com/lnorton89/rustdirstat/commit/55bc645da6dae3b0e404e871d9990fa1547bcee2))

### Changed

- satisfy the stricter lint set ([`7b3774c`](https://github.com/lnorton89/rustdirstat/commit/7b3774c87461bd3c27780c4421227e0dd9e7a6ea))
- give Tree a placeholder constructor ([`dee858f`](https://github.com/lnorton89/rustdirstat/commit/dee858fb9149a9f2bb8140a56f7cdffc54d39130))

### Documentation

- describe both front ends, and add notes for future work ([`6e6aaf0`](https://github.com/lnorton89/rustdirstat/commit/6e6aaf0d521ea4b59550c90e42355941bbeaabab))

### Internal

- add cross-platform CI and tagged release packaging ([`c92cc9b`](https://github.com/lnorton89/rustdirstat/commit/c92cc9ba065fc9455625997f01016ad105caab94))
- make every target lint-clean under the crate's own rules ([`c4c1cf9`](https://github.com/lnorton89/rustdirstat/commit/c4c1cf93f5b0ddadc96372500c6f369c683830bd))

### Tests

- drop the blanket lint exemption for test code ([`8629685`](https://github.com/lnorton89/rustdirstat/commit/86296859e56a8a5a7c9b34193b47fae83131b62e))
- make the pty stress test portable and self-explanatory ([`b56328a`](https://github.com/lnorton89/rustdirstat/commit/b56328a93045b2d139cadae0dbe8e6818abd5cb7))

### Other

- Add a real desktop GUI (egui/eframe) alongside the TUI ([`37a0802`](https://github.com/lnorton89/rustdirstat/commit/37a0802a08df26e3480827ecd893d7b6940ea311))
- Add cushion-style per-cell tile shading to the treemap ([`8ac31bd`](https://github.com/lnorton89/rustdirstat/commit/8ac31bd6b03fd6c5405131bd912b025a02619277))
- UI review round 8: filter state leaking across views, report truncation, weaker test assertion ([`758f998`](https://github.com/lnorton89/rustdirstat/commit/758f99830f2886a11ef3a203d858a2f4a4e11c22))
- UI review round 7: symlink-move correctness, dup-detection determinism, progress batching ([`6f10a11`](https://github.com/lnorton89/rustdirstat/commit/6f10a11a01708b1e95697ecf7bf965e0e573145f))
- UI review round 6: wide-character overflow, resize/click race, CSV \r escaping ([`e752307`](https://github.com/lnorton89/rustdirstat/commit/e75230774bfbf602e38acaa23354e203cdc97b45))
- Fix squarified treemap producing gaps and overlaps between sibling tiles ([`5b77408`](https://github.com/lnorton89/rustdirstat/commit/5b774084144293222c5192191b749523f21f89ef))
- UI review round 4: stale selection after duplicates view, treemap label overwrite, physical-size legend gap ([`e0cb650`](https://github.com/lnorton89/rustdirstat/commit/e0cb650e44520f1255065f512de1f71478fc86f8))
- UI review round 3: header can still overflow, missing title click zones, undersized prompts ([`4d684be`](https://github.com/lnorton89/rustdirstat/commit/4d684bec2067703f4038e80dd374f3510c18eae1))
- UI review round 2: fix click-zone/render desync and color-collision bugs ([`d250b4d`](https://github.com/lnorton89/rustdirstat/commit/d250b4dc3c3ee7f0a7c583b59d92c06a52654be0))
- UI consistency pass: theme, highlighting, header truncation, legend wrap ([`70e8af4`](https://github.com/lnorton89/rustdirstat/commit/70e8af4f77da763b09e49b3b7aee9cce1913266d))
- Color treemap tiles per-extension instead of per-category ([`4b2f293`](https://github.com/lnorton89/rustdirstat/commit/4b2f293f760e26d0888122ea779805af0dc542b8))
- Categorize compiled build artifacts, fixing near-monochrome build-dir treemaps ([`cac1c09`](https://github.com/lnorton89/rustdirstat/commit/cac1c0964a769ff233855bd763671f86025c293b))
- Fix treemap illegible label soup on dense trees (build output, node_modules) ([`c1cc96a`](https://github.com/lnorton89/rustdirstat/commit/c1cc96ad87cacdc80de8387f405d84457670511d))
- Add Windows system-maintenance tools menu ('T'), cfg-gated but always visible ([`1daf8e9`](https://github.com/lnorton89/rustdirstat/commit/1daf8e924c7feac908171056492b614e5feabfff))
- Add file actions: direct Open, reveal in file manager, Copy path, Move to, Properties ([`f883624`](https://github.com/lnorton89/rustdirstat/commit/f883624a064ad72e57b479e614dc6b5900e146a5))
- Fix free-space treemap tile swamping small subfolder scans; add Empty folder ([`61bcba0`](https://github.com/lnorton89/rustdirstat/commit/61bcba04192888845e1a9a60d695a851e2027753))
- Fix real crossterm event-loss bug causing quit to hang; add regression test ([`e049fca`](https://github.com/lnorton89/rustdirstat/commit/e049fca663d8c949fa82a0958f84f4f1030c34b3))
- Fix treemap starving sibling directories on large real-world scans ([`da2159f`](https://github.com/lnorton89/rustdirstat/commit/da2159ffe3c04f42be9670f820cea726d0c614ed))
- Add CSV export: --csv CLI flag and in-TUI 'E' key ([`755b38f`](https://github.com/lnorton89/rustdirstat/commit/755b38f4353f377f159417d95ed397626524a58f))
- Persist sort order, treemap state, and size-mode preferences across runs ([`71ecdae`](https://github.com/lnorton89/rustdirstat/commit/71ecdae44f573473538384f4442529d69e31d4ef))
- Add duplicate file detection ('u'), matching WinDirStat's Duplicate Files view ([`5b5dc09`](https://github.com/lnorton89/rustdirstat/commit/5b5dc09c12e7e5af2ee4c2005f76d153c6a3805d))
- Add free-space treemap tile and recursive subtree search (glob/regex) ([`01c6322`](https://github.com/lnorton89/rustdirstat/commit/01c63220f13a89646bdec4db208acba69fc37569))
- Fix Ctrl+C never quitting; add panic-safe terminal restore; add physical size ([`28e35a1`](https://github.com/lnorton89/rustdirstat/commit/28e35a1aa302c1616b547d0b0789ca1775930db5))
- Fix root cause of treemap still reading as one color; visible resize handle; stop silently dropping scan errors ([`cf35e0f`](https://github.com/lnorton89/rustdirstat/commit/cf35e0f2c7e7eeb49d0404f290ca30098fb6d73c))
- Fix mouse-motion event flood ("won't quit"); treemap readability ([`2c17861`](https://github.com/lnorton89/rustdirstat/commit/2c178618300072d9bd57e1fd3488e5df50017eb6))
- Declutter the UI: less color noise, fewer buttons, bigger treemap tiles ([`8a62cdb`](https://github.com/lnorton89/rustdirstat/commit/8a62cdbc3eb8ff10bd68a183659f9989adbfc0ab))
- Major performance overhaul, GUI-style theming, and full feature set ([`de7b874`](https://github.com/lnorton89/rustdirstat/commit/de7b874b90f1fdada7617969cd9fd5e20e460150))
- Recursive nested treemap + full mouse support ([`7c6f176`](https://github.com/lnorton89/rustdirstat/commit/7c6f176be72fac3e76a0264b8519890b5c202406))
- Add rustdirstat: cross-platform terminal clone of WinDirStat ([`79c6d06`](https://github.com/lnorton89/rustdirstat/commit/79c6d0697c7cad167dbe1ca6bf7ccfcded8df75f))

[Unreleased]: https://github.com/lnorton89/rustdirstat/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/lnorton89/rustdirstat/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/lnorton89/rustdirstat/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lnorton89/rustdirstat/releases/tag/v0.1.0
