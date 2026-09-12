# Shipping publicly — legal posture (not legal advice)

Question (2026-09-12): can we ship a product this specific to hardware we hold
no rights in? Answer: yes — third-party companion software is an established,
legal category — provided the hygiene below. Consult a lawyer before any
*commercial* launch.

## Why the core activity is fine

- **Protocol interoperability is protected**: EU Software Directive Art. 6 and
  US precedent (Sega v. Accolade; DMCA §1201(f)) cover reverse-engineering for
  interoperability, as long as it is wire-observation / clean-room — never
  decompiling Elektron code. rytmdash only observes sysex and uses community
  specs.
- **Direct precedent, same device family, years public, no takedowns**:
  elektroid, libanalogrytm, rytm-rs, Elk-Herd, MegaCommand, STROM (sold on the
  App Store). Elektron has historically tolerated this ecosystem.
- **Facts are free**: CC/NRPN numbers, parameter names, slot counts extracted
  from the manual are uncopyrightable facts. The manual PDF itself is not —
  which is why `reference/` stays gitignored, never committed or redistributed.

## Risk checklist before public release

1. **Product name (the one real exposure)**: "rytmdash" contains Elektron's
   "Rytm" trademark. Saying "works with Elektron Analog Rytm MKII" in a
   description is nominative fair use; the mark *inside the product name* is
   weaker ground. Decide: rename, or accept that a trademark letter would
   target this first.
2. **Disclaimer everywhere the product speaks** (app about-box, README, site):
   "Not affiliated with or endorsed by Elektron. Elektron, Analog Rytm and
   +Drive are trademarks of Elektron Music Machines AB." No Elektron logos.
3. **License hygiene** (already policy): elektroid is GPLv3 → spec only, never
   ported code (CLAUDE.md hard constraint). rytm-rs / rytm-sys / libanalogrytm
   are permissive → attribution in a NOTICE/THIRD-PARTY file at release.
4. **Never redistribute Elektron content**: no factory samples/Sounds, no
   firmware, no bundled device dumps. The app moves only the user's own data.
5. **Warranty/liability**: ship AS-IS disclaimer (MIT-style); the backup-first,
   plan/preview/verify write pipeline is the substantive protection against the
   realistic harm (corrupting a user's project).
6. **Business (not legal) risk**: Elektron owes third parties nothing — a
   firmware update can break the protocol any day. Mitigation is the existing
   firmware-gate discipline (docs/hardware-verification.md) and pinning
   verified-OS versions per write path.

## Release-gate summary

Before the first public build: rename decision (item 1), disclaimers (2),
NOTICE file (3), AS-IS license text (5). Items 4 and 6 are standing policy.
