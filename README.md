# RAMP

RAMP connects Maverick OS hardware, AIR services, and Prism's application interfaces.

## Building

Maverick builds FFmpeg automatically through Cargo and links its libraries statically into the
application. FFmpeg 9.0.2 source ships in the transitive binding crate and is compiled for the
target. An installed FFmpeg package or runtime is not required. Building FFmpeg requires
neither Git nor a separate source download.

macOS and iOS builds use pregenerated FFmpeg bindings and require Rust, Xcode's native compiler
and target SDK, and Make. NASM is built automatically from bundled source only for Intel
(`x86`/`x86_64`) targets, preserving FFmpeg's optimized assembly. No separate NASM installation
or build-time source download is needed. These builds do not require libclang or pkg-config.
Subsequent builds reuse Cargo's build cache.

Run the application demo with:

```sh
cargo run --features demo
```

## Notifications

Enable notifications directly on an AIR instance:

```rust
instance.notify(true);
instance.notify(false);
```

Notifications default to false when an instance is opened. The flag is shared by all clones of
that instance and is independent of other instances. Calling `notify(false)` removes its remote
watch. The normal instance service handles registration, acknowledgments, and local alerts;
there is no separate application notification client or per-instance notification service.

Only live incoming updates after catching up to the channel head produce local alerts. Historical
replay and confirmation of the current user's own writes do not alert. In the foreground, Maverick
sends `Input::Notification(header, body)`, which RAMP broadcasts as `prism::events::Notification`.
In the background, Maverick posts a native device notification. The built-in alert is generic;
Maverick's `hardware.notifier()` returns a clonable `Notify` handle for custom local content through
`notify.notify(header, body)`. Services receive that handle during setup and do not retain Hardware.

RAMP holds one shared `Instance<Notifications>` per AIR identity, installation, and application
configuration. It uses the same AIR instance and inbox services as other contracts. Its only
additional worker observes the hardware push destination and updates that contract on token
rotation; the token and signing credentials stay out of Prism.

At each channel head, RAMP registers the next public file key with the notification server.
The watch remains armed while the app is online. After handling an update, RAMP acknowledges
that file. The server delays an available file's alert by 500 ms and cancels it if the owner's
acknowledgment arrives first. Late acknowledgments cannot remove a newer watch. A delayed or
lost acknowledgment can still produce a duplicate device alert; the grace period is best effort.
App suspension needs no background WebSocket guarantee: an armed remote watch survives it.

### Application configuration

The `air_notifications` crate owns the notification contracts, its `PushDestination`, and the
standalone server. It has no dependency on Maverick, Prism, or RAMP. RAMP re-exports its contracts
as `ramp2::notifications` and converts Maverick's destination in the existing push service.

Create the application configuration through a normal AIR instance:

```rust
use ramp2::notifications::{ApnsCredentials, NotificationApplication, Notifications};

let mut application = air.instances::<NotificationApplication>().create((bundle_id, sandbox));
application.share(Notifications::server());
assert!(application.send(ApnsCredentials {
    key: std::fs::read_to_string(key_path).unwrap(), key_id, team_id,
}).confirmed().await);
let application_id = application.id();
```

Cargo Piston can use the same contract directly through AIR, then compile the app with
`RAMP_NOTIFICATION_APPLICATION=<application_id>`. Only this public ID belongs in the built app;
the signing key and contract's private location stay in the deployment tool and server.
The developer can rotate credentials on that instance without changing its ID. Sandbox and
production use separate instances. AIR confirmation does not verify credentials with Apple.

Each device registration initializes `Notifications` with `(installation_id, application_id)`.
RAMP supplies the current destination and updates it when APNs or FCM replaces the token. A build
without `RAMP_NOTIFICATION_APPLICATION` provides local notifications only.

For Android, create an `FcmApplication` instead of the Apple application contract:

```rust
use ramp2::notifications::{FcmApplication, FcmCredentials, Notifications};

let mut application = air.instances::<FcmApplication>().create((project_id, package_name));
application.share(Notifications::server());
assert!(application.send(credentials).confirmed().await); // FcmCredentials from the server key
let application_id = application.id();
```

Build with that ID in `RAMP_NOTIFICATION_APPLICATION`. Package Maverick's Android Gradle bridge
and the matching Firebase `google-services.json` resources. `instance.notify(true)` uses the same
watch/acknowledgment flow on Apple and Android; no Android-specific Prism API is required.
The shared notification server needs the FCM update before accepting Android registrations.

Run the standalone server from the `air_notifications` project with `cargo run --features server`.
Its deployment files and server documentation live there. RAMP only has the `demo` feature;
Maverick's hardware is always available. Neither crate has a special server-only build mode.

Frontend dependencies use sibling paths. Backend dependencies use published crate versions;
`.cargo/config.toml` retains only frontend overrides. Apple push entitlements, signing, and notification
permission are still required for actual device delivery.

## Change futures

Ramp forwards Maverick's `Application::changed().await` to `Prism::changed().await` and
`Application::draw()` to `Prism::draw()`. Air, picker, and playback adapters await their native
handles directly. Task contexts and wakers are confined to Maverick's window-loop adapter.

Front-end components declare separate `#[on_change(source_expression)]` implementations of
`OnChange<Update>`. Prism generates the distinguishing tag and registration, then combines
independently borrowed sources and children into one `changed()` future per component.
The completed future carries a deferred `Change`, applied after competing waits are dropped.
Use `ChangeSource::changed()` for optional sources that should stay asleep when inactive.
