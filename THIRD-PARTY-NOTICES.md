# Third-Party Notices

aegis bundles and (for OpenCode) rebrands the following third-party software. Their licenses
and copyright notices are retained here as required. aegis-cli itself is Apache-2.0 (see
`LICENSE` / `NOTICE`).

---

## OpenCode (anomalyco/opencode)

The aegis TUI is a build-time-hardened, **rebranded** build of OpenCode
(<https://github.com/anomalyco/opencode>). The MIT license permits this; the original license
and copyright are retained below. aegis applies only minimal, reviewable build-time patches
(`deploy/opencode/patches/`) over a pinned upstream revision (`deploy/opencode/OPENCODE_REF`) —
it is not a fork and does not reimplement the harness.

```
MIT License

Copyright (c) 2025 opencode

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
```

---

## llama.cpp (ggml-org/llama.cpp)

The bundled `llama-server` is built from llama.cpp (<https://github.com/ggml-org/llama.cpp>),
MIT License, Copyright (c) 2023-2024 The ggml authors. The full MIT text matches the OpenCode
license above (same terms; different copyright holder).

## ripgrep (BurntSushi/ripgrep)

The bundled `rg` is ripgrep (<https://github.com/BurntSushi/ripgrep>), dual-licensed
**MIT OR Unlicense**, Copyright (c) 2015 Andrew Gallant. aegis uses it unmodified (pinned +
checksum-verified, see `deploy/opencode/RIPGREP_REF`).

## rtmx (rtmx-ai/rtmx)

The bundled `rtmx` intent engine is part of the rtmx-ai project; see its repository for its
license and copyright.
