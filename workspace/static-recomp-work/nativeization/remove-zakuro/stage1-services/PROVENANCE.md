# Native CTR services migration slice

Actual donor service code remains in TriAevum `tools/oot3d/source_native_runtime`.
CFG, APT and NDM are generic profile/state extensions; no OoT gameplay data.
The Y2R session links `tools/ctr_services/y2r_service.h` directly, retaining
its Citra/Azahar copyright and GPL-2.0-or-later notice. No Zakuro conversion
function is called by this session.

Behavioral references:
- https://3dbrew.org/wiki/Config_Savegame
- https://3dbrew.org/wiki/Cfg:GenHashConsoleUnique
- https://3dbrew.org/wiki/Cfg:GetRegionCanadaUSA
- https://github.com/azahar-emu/azahar/blob/master/src/core/hle/service/ndm/ndm_u.cpp
- https://github.com/azahar-emu/azahar/blob/master/src/core/hle/service/ndm/ndm_u.h

Virtual console defaults are explicit typed configuration, not arbitrary
zero-filled unknown blocks. Unknown blocks/commands reject without reference
fallback. Console hash identity is persistent per copied native FS profile,
randomly generated once, using real SHA-256 and CTR's 20-bit application salt.

Validation: `test-services.py` exercises the actual shared-library service
objects, configuration sizes/descriptors/address failures, SHA output, nested
daemon suspension, exclusive ownership, APT queue/peek/consume, and real YUV420
conversion/resource writes/completion event signaling.

Runtime evidence: `run-14.log`, `field-cfg-apt-ndm-y2r.png` (saved field loaded
and player walked; dual screen, native audio produced nonzero PCM).
Native calls 47033; reference calls 2383; rejected commands 0.

Later validation: run-17.log and field-all-services-native.png load the
existing saved field, walk, and quit in dual-screen mode. Native service calls
57289, reference service calls 0, rejected commands 0. The native directory
rejects one unregistered port probe (unknown_ports=1); detailed name logging
and propagation into unknown_service_calls were added after this smoke.

Friends uses typed empty local account/presence/list state, not arbitrary
unknown reply buffers. SSL Initialize and GenerateRandomData use native
getrandom entropy and bounded mapped buffers. UDS returns the actual wireless
off error. References: Azahar frd.cpp and 3dbrew Friend Services/UDS.
BOSS owns initialization identity; GetStorageEntryInfo returns an explicit
native offline-profile no-storage policy error, NOT a claim of the exact
firmware result. It does not fabricate a successful storage record, use the
old service dispatcher, or attempt networking. Result field definitions:
https://github.com/devkitPro/libctru/blob/master/libctru/include/3ds/result.h

The independent pokemon3ds-runtime Rust crate owns native service directory,
BOSS local state and module/CRR registry. Generic CRO format/rebasing algorithms
were extracted from Zakuro MIT sources (copyright/license retained), adapted
to checked GuestMemory/ProcessMemory contracts, with no Zakuro dependency in
that crate. TriAevum has no portable CRO/CRR loader, so this narrow algorithm
donor fills that gap; no game-derived generated code or assets are imported.
CRR SHA256 module membership and title identity are enforced. Nintendo RSA
CRR signatures are not verified: explicit trusted-local-dump policy.
Native loader smoke run-16 and run-17: 9 commands, 1 CRR, 5 CRO loads, zero
reference CRO calls. Five source-only Rust tests cover native static ownership,
table/certificate bounds, title mismatch, unattested modules, directory
registration/notifications, and no fabricated BOSS storage success.

Service state/dispatch is native on this smoke path; kernel handle namespace,
synchronization, scheduler/bootstrap/platform still require subsequent stages.
APT/Y2R native kernel objects still have transitional reference-kernel mirrors
pending Stage 2. No zero-Zakuro-link success is claimed.
