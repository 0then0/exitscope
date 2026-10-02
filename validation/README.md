# Validation inputs and selected evidence

`upstream-sources.json` is the maintainer-managed manifest consumed by
`scripts/external.py`: pinned URLs, upstream commits and SHA256 digests.
`upstream-evidence.json` preserves compact upstream comparison metadata;
this is external-target evidence, not external adoption.

Generated reports, stdout/stderr and batch directories are ignored by Git.
Validation still saves every outcome locally; CI uploads per-architecture artifacts
with `always()`. GitHub artifacts expire, so they are not the permanent archive.

The complete historical results, including unsuccessful runs and infrastructure
errors, remain in the [public evidence snapshot](https://github.com/0then0/exitscope/tree/0f700241356d8a8825d0f620fc45793057de02ac/validation).
The [snapshot archive](https://github.com/0then0/exitscope/archive/0f700241356d8a8825d0f620fc45793057de02ac.tar.gz)
contains the whole repository, including those results. History was not rewritten.
The engineering report links historical files directly to that commit.

## Selected deadline reports

These four unmodified reports preserve the original false PASS and corrected
FAIL/PASS controls. They are recorded local native ARM64 executions, not new runs
or hosted x86_64 evidence. Full commands and environments remain in the snapshot.

- [baseline-late-receipt.json](examples/baseline-late-receipt.json): [original report](https://github.com/0then0/exitscope/blob/0f700241356d8a8825d0f620fc45793057de02ac/validation/v0.1.1/arm64/baseline/receipt-late.json), SHA256 `b133c6863234ddc1cb039f22cd659401b281519527caa547e56563fb27bfd9d3`.
- [late-receipt.json](examples/late-receipt.json): [original report](https://github.com/0then0/exitscope/blob/0f700241356d8a8825d0f620fc45793057de02ac/validation/v0.1.1/arm64/observer-final/1790951785365586841/receipt-late.json), SHA256 `ba78d32e1cda81c817c5902cf861fd8991e2230768cf11e624f7c0acee829b4d`.
- [timely-receipt.json](examples/timely-receipt.json): [original report](https://github.com/0then0/exitscope/blob/0f700241356d8a8825d0f620fc45793057de02ac/validation/v0.1.1/arm64/observer-final/1790951785365586841/receipt-timely.json), SHA256 `bd99a1401675b59ec2f82896357dbfd99a31bae6aa35d203f5a3c04141ffa283`.
- [disabled-receipt.json](examples/disabled-receipt.json): [original report](https://github.com/0then0/exitscope/blob/0f700241356d8a8825d0f620fc45793057de02ac/validation/v0.1.1/arm64/observer-final/1790951785365586841/receipt-disabled.json), SHA256 `0f477343559fb7ca407e2f5fe183e5710115cdae7ee8a175a67c40b3f3a7236a`.
