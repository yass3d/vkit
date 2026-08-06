# License

Copyright 2026 Vkit contributors.

Vkit is dual-licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option. `SPDX-License-Identifier: MIT OR Apache-2.0`.

Contributions submitted for inclusion in this work are dual-licensed the same
way, with no additional terms.

## The author's position, in plain words

This is a hobby project, written for the interest of writing it and given away.
The author claims nothing over it and promises nothing about it.

- **Take it.** Use it, read it, change it, fork it, sell what you make with it.
  You owe no credit, no notice, no payment and no permission. The one thing the
  licences ask is that a copy of the licence text travels with a copy of the
  code, and only because the law needs *something* written down before it will
  agree you were allowed to have it.
- **It is given as it is.** There is no warranty of any kind: not that it works,
  not that it will keep working, not that it is fit for anything in particular.
  Both licences say this in capital letters and they mean it.
- **What you do with it is yours.** If it loses your work, exports something
  wrong, corrupts a file, or gets you in trouble with somebody whose content you
  ran through it, that is between you and the consequence. The author is not
  liable for it, is not obliged to fix it, and owes no support.
- **Your inputs stay your problem.** Vkit reads morphs, skins, hair and looks
  that belong to other people and writes files derived from them. Their terms
  follow the output. Nothing here grants you rights over Virt-A-Mate or DAZ 3D
  content, and nothing here can.

If you would like it to keep being free for the next person, pass it on the same
way. That is a request, not a condition — see below for why it cannot be one.

## Why these licences and not a Creative Commons one

A few things are worth stating because they are commonly assumed backwards.

**MIT and Apache-2.0 already *are* the "no rights, no responsibility" licences.**
That is the whole of what a permissive licence does: it hands over every use and
disclaims every promise. Apache-2.0 says it at more length — an explicit warranty
disclaimer in §7, an explicit limitation of liability in §8, and a patent grant
that MIT lacks — which is why it is offered first here. Choose either.

**Creative Commons is the wrong family for software, and its restrictive
variants are the opposite of this intent.** Creative Commons itself recommends
against using CC licences for software. `NC` (non-commercial) and `ND` (no
derivatives) are *restrictions*: they would forbid selling anything built with
this and forbid modifying it, which is the reverse of what is meant above. `CC0`
is the genuine "no rights reserved" dedication and would be defensible here, but
it carries no patent grant and is unusual enough in software that packaging
tools and corporate reviewers stumble on it. The dual MIT/Apache pair is what the
Rust ecosystem uses, is what every dependency here uses, and costs the user
nothing extra.

**"No rights claimed" and "everyone downstream must stay free" cannot both be
true.** Requiring anything of a person who redistributes your work — even
requiring that they keep it free — means holding copyright and enforcing it
against them. That is what copyleft is, and it is a claim of rights, not a
disclaimer of them. This project chose the disclaimer. Somebody may take Vkit,
close the source, and sell it; that is a real consequence of this licence and it
is accepted deliberately.

**A disclaimer of liability is not the same as a disclaimer of copyright.** The
"no responsibility" half above works because it is written into the licence
terms, not because the author waived ownership. This is the reason to use a real
licence rather than declaring the work public domain in a README.

## Third-party components have their own terms

Those terms are not disclaimed by anything above, and a redistributor has to
carry them:

| Component | Licence |
|---|---|
| Rust dependencies (pinned by `native/Cargo.lock`) | MIT OR Apache-2.0, except the rows below |
| `dyn-eq`, reached through tract | MPL-2.0 — source at <https://github.com/Rayzeq/dyn-eq> |
| `tiny-skia`, `tiny-skia-path` | BSD-3-Clause |
| `moxcms`, `pxfm` | BSD-3-Clause OR Apache-2.0 — Apache-2.0 taken |
| `arrayref` | BSD-2-Clause |
| `libloading` | ISC |
| `adler32`, `foldhash` | Zlib |
| `clipboard-win`, `error-code` | BSL-1.0 |
| `hexf-parse` | CC0-1.0 |
| `unicode-ident` | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `self_cell` | Apache-2.0 OR GPL-2.0-only — Apache-2.0 taken |
| `dpi` | Apache-2.0 AND MIT |
| `approx`, `codespan-reporting`, `flatbuffers`, `nalgebra`, `simba`, `unicode-general-category`, `winit` | Apache-2.0, with no MIT alternative |
| 31 crates — `harfrust`, `rfd`, `nom`, `tracing`, `strum`, `lz4_flex`, `libm` and the rest | MIT, with no Apache-2.0 alternative (four of them offer the Unlicense as the other option). Named one by one, with the copyright line each of them requires, in `build/windows/THIRD-PARTY-NOTICE.txt` |
| Vendored `tract-tflite` | MIT OR Apache-2.0 — `native/vendor/tract-tflite/` |
| Lucide icon set | ISC, and MIT for the glyphs Lucide inherited from Feather |
| MediaPipe Face Landmarker models | Apache-2.0 — `native/vkit_semantic/MODEL_PROVENANCE.md` |
| LSMR solver | algorithmic port from SciPy, BSD-3-Clause |

The dependency rows are the set that reaches the shipped executable —
`cargo tree -p vkit-app -e normal` for the Windows x64 target — and not the
dev-dependencies or the build script's own.

`build/windows/THIRD-PARTY-NOTICE.txt` is compiled into the executable, and
every licence named above is reproduced there in full, so a copy of the binary
carries all of them on its own. One exception, named rather than left to be
found: `CC0-1.0` is a dedication to the public domain rather than a licence. It
asks nothing of a redistributor and `hexf-parse` ships no licence file and names
no holder, so there is no text for either document to carry.

## Not affiliated

Vkit is an independent project and is not affiliated with, endorsed by, or
supported by Meshed VR or DAZ 3D. It requires a Virt-A-Mate installation that
you already licensed, and it redistributes none of one.

Virt-A-Mate and VaM are Meshed VR's marks; Genesis and DAZ are DAZ 3D's. This
repository names them where it has to say what a file is or what the program
reads and writes, which is the use trademark law allows and the only use made
of them here. Nothing the project says about itself is built on either name.

The executable carries two pieces of figure geometry and both are the project's
own: 18 KB of eyelid displacement that drives the open/closed eye preview, and
372 KB of mouth and expression shapes, seven of them, offered through the
Built-ins tab. Each holds displacements and a topology digest and nothing else
— no positions, no polygons, no UVs — so no cage can be recovered from either.
The seven are named after the DAZ built-ins they stand in for, because that is
the name a VaM installation looks them up by; the name is DAZ's and the shape
is not. Neither reaches a VaM installation unless you install it from the
Built-ins tab, which backs up what it replaces.
`build/windows/THIRD-PARTY-NOTICE.txt`, compiled into the executable, says the
same thing to anyone holding only the binary.

