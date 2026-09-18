<!--
  SPDX-FileCopyrightText: 2026 Kubuno contributors
  SPDX-License-Identifier: AGPL-3.0-or-later
-->

<div align="center">

<img src=".github/logo.png" alt="Kubuno Books logo" width="120">

# Kubuno — Books

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
![Rust](https://img.shields.io/badge/Rust-edition_2021-orange.svg)
![React](https://img.shields.io/badge/React-19-61dafb.svg)
![Status](https://img.shields.io/badge/status-alpha-yellow.svg)
![Module](https://img.shields.io/badge/Kubuno-module-4D38DB.svg)

**A self-hosted library for your books, comics and eBooks on Kubuno — scan your folders, enrich the metadata, and read in the browser or from any OPDS reader.**

Books is a module for [Kubuno](https://github.com/kubuno/core), the self-hosted, libre (AGPLv3) cloud platform — a sovereign alternative to Google Workspace and Microsoft 365. Point it at the folders where your files already live and it builds a browsable catalog with covers, series, reading progress and an OPDS feed, all served from your own instance.

</div>

---

## ✨ Features

- 📖 **Many formats** — EPUB eBooks and PDF, plus comic archives in CBZ, CBR and CB7, with covers and page counts extracted on scan.
- 🗄️ **Libraries from your folders** — register libraries backed by folders, scan them on demand (with live scan status) and re-scan to pick up new files; the library is an index over files you keep, not a second copy.
- 🧭 **Organize** — series, collections, read-lists, saved searches, faceted filtering, duplicate detection, "recent" and "keep reading" shelves.
- 🏷️ **Metadata** — read embedded metadata (including `ComicInfo.xml`), search and apply metadata from online providers per book or per series, and refresh a whole library at once; a default metadata language is configurable.
- 📚 **In-browser reader** — read page by page in the browser, with per-user reading progress tracked and synced, and mark titles read or unread.
- 📡 **OPDS catalog** — an OPDS feed (recent titles and series) that external reading apps can browse and download from; it can be turned off entirely when unused.
- ⬇️ **Downloads & export** — download original files (which an administrator can disable, keeping browser reading intact) and export catalog data.
- 🔞 **Access & restrictions** — per-account access policy: restrict which libraries an account may see and enforce a maximum age rating, optionally hiding unrated content and importing the `AgeRating` tag from `ComicInfo.xml`.

## 🏗️ Architecture

Like every Kubuno app, Books is an **independent process**, not a library linked into the core. It registers with the [core](https://github.com/kubuno/core) at startup; the core then proxies its routes (`/api/v1/books/*`), distributes platform events to it, serves its runtime-loaded React frontend bundle and manages its lifecycle.

- **Port** — the backend listens on `127.0.0.1:3121` and is reached only through the core's reverse proxy.
- **Backend** — `src/`: Axum + SQLx over PostgreSQL, confined to the `books` schema; migrations in `migrations/`. Background workers scan libraries and fetch metadata; the module publishes book add/delete events on the platform bus and reacts to account deletions.
- **Frontend** — `frontend/`: a React 19 bundle built to `entry.js` + `entry.css`, consuming `@kubuno/sdk`, `@ui` (`@kubuno/ui`) and `@kubuno/drive`. At runtime those specifiers are `external` and resolved by the host's import map to its single shared instances; the npm packages are used only for building and type-checking.
- **Trust boundary** — proxied requests are authenticated from a signed `X-Kubuno-Auth` token minted by the core (see `kubuno-modauth`), never from plain `X-Kubuno-User-*` headers.

## 📦 Install

The easiest way to self-host a full Kubuno instance (core + every module) is the **all-in-one Docker image** (`ghcr.io/kubuno/kubuno`), which already bundles this module — see **[kubuno/docker](https://github.com/kubuno/docker)** for `docker compose` instructions.

To add the module to an existing instance, install its **`.kbpkg`** — the single, cross-platform package format a Kubuno server unpacks by itself (no `.deb`/`.rpm`/`.exe`/`.pkg`, and no external tools). Each tagged release (`v*`) attaches a Linux `.kbpkg` (built by `build.yml`) and Windows/macOS `.kbpkg` files (built by `dist.yml`) to its [GitHub Release](https://github.com/kubuno/books/releases):

```bash
# From the admin console: Modules → Install, then drop the .kbpkg — or, offline, from the CLI:
sudo kubuno modules:install dist/books-<version>-<os>-<arch>.kbpkg
sudo systemctl restart kubuno     # the core loads the module on (re)start
```

## 🛠️ Build & development

**Requirements:** Rust ≥ 1.82, Node.js ≥ 24, PostgreSQL 16. No `kubuno/core` checkout is needed — shared Rust crates come from tagged git dependencies, and the `@kubuno/*` frontend libraries from the public npm scope.

```bash
cargo build --release                     # → target/release/kubuno-books
cd frontend && npm ci && npm run build     # → dist/{entry.js, entry.css}

bash build_kbpkg.sh                        # → dist/books-<version>-<os>-<arch>.kbpkg
bash build_kbpkg.sh --install              # build, install into the module store, restart
```

Once the module has been installed at least once, iterate quickly without repackaging:

```bash
bash ../_tools/deploy_local.sh books             # backend + frontend
bash ../_tools/deploy_local.sh books --frontend  # frontend only (fastest)
```

## 📦 Tech stack

Rust 2021 · Axum 0.7 · Tokio · SQLx 0.8 (PostgreSQL 16, schema `books`) · background scan & metadata workers · OPDS — React 19 · TypeScript · Vite · Tailwind CSS v4 · Zustand · React Query, on the shared `@kubuno/sdk`, `@ui` and `@kubuno/drive` surfaces.

## 🤝 Contributing

Contributions are welcome. Please open an issue to discuss any significant change before submitting a pull request.

## 📄 License

[AGPL-3.0-or-later](LICENSE) © Kubuno contributors.
