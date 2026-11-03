# Receive-only radio session kernel

Implemented slice of [#32](https://github.com/mESmaC/oia/issues/32),
[RADIO-01/02](../spec.md) and the capability/status subset of
[A11](../delivery.md#a11-radio-sources-and-audio-evidence).
The Rust [module](../../crates/oia-devices/src/radio.rs) validates adapter declarations
and projects session state without I/O. It does not establish receiver support or reception.

Directory listings cannot start reception. Audio streams cannot declare tuning or RF
spectra. Receiver declarations can expose bounded integer-Hz tuning and RF spectra;
audio spectra remain explicitly audio, including on receivers. A waterfall is only a
display style and does not change the spectrum's meaning. Declarations are supplied by
trusted application adapters, not authorization for network or device access.

Source and version IDs follow every readout; simulated receivers remain labeled.
A validated tuning request is separate from observed tuning. Missing observations,
frequency and latency remain absent. Receipt times are milliseconds on one monotonic
session clock, not capture times or UTC. Adapters must document the clock method for
any supplied latency. A readout is Receiving only while the last observation's age is
less than the configured positive stale threshold; at the threshold it is Stale.
Connecting means no observation has arrived, even after a long wait; the application
job/transport owns connection deadlines. Reading a status performs no mutation or retry.

Disconnect and access restriction permanently end that session. Last observations remain
historical; late observations and tuning requests are rejected. Reconnection requires a
new adapter-owned session. An adapter must route completions to their originating session.
Event timestamps may be equal, but cannot regress. Invalid operations preserve prior
state. Memory is constant apart from bounded IDs and at most five modulation entries.
There is no transmission operation, endpoint, credential, automatic fallback or reconnect.

Run the deterministic example (generated observations; no audio or RF data):

```text
cargo +1.99.0 run -p oia-devices --example radio-session --locked --offline
cargo +1.99.0 test -p oia-devices --test radio_contract --locked --offline
```

Three compact contract tests address false receiver capability claims, request/observation
separation and atomic validation, and stale/late data publication. These are A11 fixture
evidence, not full A11 acceptance or a Lean proof.

Remaining #32 scope: directory discovery and source registry/service integration, geographic
roles, stream transport and actual audio playback, persistent favorites, receiver restrictions
and explicit local registration, signal measurements, squelch, clips/bookmarks and original
retention, clock alignment, uncertain transcription/translation, exports and UI. D08 production
receiver/protocol selection remains open. Live stream and actual supported receiver checks
are required before end-to-end or hardware claims. This slice adds no production dependencies
and uses no unmerged prerequisites or prior-art code.
