# Nebby licensing

## Original Nebby code — MIT

The following MIT license applies to original Nebby launcher contributions, not
to third-party code, game code, or game assets. The same license is retained in
[`LICENSE`](LICENSE) for tooling compatibility.

MIT License

Copyright (c) 2026 Nebby contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Third-party runtime and tools

Component licenses and copyright notices remain in force:

- **3dsrecomp:** MIT, as declared by its upstream Cargo.toml. Available upstream
  metadata is recorded in [`licenses/3dsrecomp/NOTICE.md`](licenses/3dsrecomp/NOTICE.md).
- **Zakuro:** MIT; preserve its license and source notices.
- **TriAevum:** GPL-3.0-or-later for its original contributions, with separately
  licensed donor components. Its GPL-2.0-or-later Azahar-derived portions may be
  combined under GPL-3.0-or-later using their "or later" option.
- **NRI and other dependencies:** retain their respective license texts and
  notices; see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

The MIT license above does not relicense TriAevum or its donor code. A combined
native runtime containing GPL-covered components must be distributed in
accordance with the applicable GPL terms, including corresponding source.
Permissively licensed components retain their notices within that distribution.
Shipping tools alongside the launcher does not replace their individual licenses.

Complete donor texts and scope are preserved under
[`workspace/tools/external/triaevum/`](workspace/tools/external/triaevum/),
the exported runtime sources, and [`licenses/`](licenses/).

## Excluded material

This file grants no rights to Nintendo or Pokémon ROMs, firmware, keys,
trademarks, character artwork, game assets, generated retail-code translations,
or other third-party material not covered by its owner's license. Users must
provide their own compatible game data. See
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) for artwork provenance.
