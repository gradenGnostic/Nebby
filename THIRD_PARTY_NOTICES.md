# Third-party licenses and release scope

The launcher also embeds egui's default fonts via `epaint_default_fonts` 0.36.2.
These are not covered solely by the Rust crate's MIT/Apache declarations:
the package collector preserves the upstream `fonts/OFL.txt`, `fonts/UFL.txt`,
`fonts/Hack-Regular.txt` and `fonts/emoji-icon-font-mit-license.txt` under
`licenses/rust-dependencies/epaint_default_fonts-0.36.2/fonts/`.

Native service SDK builds use JSON for Modern C++ 3.11.3 (Niels Lohmann,
2013–2023, MIT). Exported headers retain SPDX/copyright notices; the SDK
also includes the installed dependency's complete copyright/license file
under `workspace/static-recomp-work/native-renderer/deps/usr/share/doc/nlohmann-json3-dev/copyright`.

Nebby's original launcher is MIT (LICENSE). This does not relicense games, character art, runtime donors or linked dependencies.

| Component | Source license declaration | Obligations to preserve |
| --- | --- | --- |
| fearkov/3dsrecomp | MIT | LICENSE and copyright notices |
| fearkov/zakuro | MIT | LICENSE and source notices; explicit experimental backend only |
| coccofresco/TriAevum | GPL-3.0-or-later, mixed provenance | LICENSE, LICENSE_SCOPE.md, LICENSES and source-file notices |
| Azahar-derived TriAevum portions | GPL-2.0-or-later | Original donor notices; see TriAevum's combined-product license scope |
| NRI | MIT | TriAevum LICENSES/NRI-MIT.txt and copyrights |
| Prism | MIT | Exported `vendor/prism/LICENSE`, source notices and local changes |
| Dear ImGui | MIT | Exported `vendor/imgui/LICENSE.txt` and separately licensed example dependencies |
| Monocypher | BSD-2-Clause / CC0 dual-license | Exported `vendor/monocypher/LICENCE.md` and individual file notices |
| BS thread-pool | MIT | Exported `vendor/threadpool/LICENSE.txt` and source notices |
| Vulkan Memory Allocator | MIT | Exported `vendor/vma/LICENSE.txt` and source notices |
| Vulkan-Headers | Apache-2.0 / MIT by file | Exported `vendor/vulkan_headers/LICENSE.md`, `LICENSES/` and SPDX headers |
| Rust dependencies | Per-package licenses | Audit locked metadata and preserve required notices per release |

Renderer donor sources and local patches are exported under the SDK's
`workspace/static-recomp-work/native-renderer/vendor/`. `PROVENANCE.json`
records Git commits where available, pinned archive declarations and exported
source-tree hashes. License metadata and corresponding-source coverage are
separate from content/path audits; preserve the supplied notices and sources.

The native renderer/game executable are not relicensed MIT by the launcher. Conveying GPL-covered binaries requires corresponding source and applicable license obligations. A link to unmodified upstream does not replace sources of local modifications. Record exact donor commits and ship notices/sources with releases. These are recorded source declarations, not a legal opinion.

Public repository/releases exclude ROMs, keys, firmware, generated recompilation C, game archives/assets, console identities and user saves. Users provide their own compatible dumps and generate game code locally. The earlier private personal-transfer ZIP is not a public release artifact.

The procedural crescent/star and library hero geometry are original Nebby code
under MIT. The launcher also includes the maintainer-selected Nebby/Cosmog icon
and SteamGridDB library artwork. Per-image source URLs and author attribution
are retained in `assets/steamgriddb/PROVENANCE.json`. These images and Pokémon
characters/marks are not covered by Nebby's MIT license. SteamGridDB hosting
alone does not establish redistribution rights; the provenance records do not
claim a license grant from the artwork owners.

The UI loads bundled artwork offline, with procedural branding as fallback.
There is no SteamGridDB API key or online artwork lookup in the launcher.
The local 3dsrecomp checkout declares MIT in Cargo.toml; see
licenses/3dsrecomp/NOTICE.md for the available upstream metadata. Other
components retain their own licenses.
