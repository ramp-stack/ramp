# RAMP

RAMP connects Maverick OS hardware, AIR services, and Prism's application interfaces.

## Building

Maverick 4.0.0 requires `ramp-ffmpeg` 9.0.4 or later, which requires `ramp-ffmpeg-sys`
9.0.5 or later. FFmpeg is linked statically; no installed FFmpeg package or runtime is needed.

iOS device and simulator builds use separate precompiled FFmpeg 9.0.2 libraries and
pregenerated Rust bindings. Cargo selects the matching device or simulator package. These
builds require Rust and the Xcode SDK/linker, but do not compile FFmpeg or NASM or run Make.

Other targets compile the bundled FFmpeg source with a native compiler and Make. macOS uses
pregenerated bindings. Intel targets outside iOS build bundled NASM automatically; no separate
NASM installation or FFmpeg source download is needed. Subsequent builds reuse Cargo's cache.

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

Frontend dependencies use sibling paths. Maverick 4.0.0 and the other backend dependencies
use published versions, so frontend builds do not require a local Maverick checkout.
`.cargo/config.toml` retains only frontend overrides. Apple push entitlements, signing, and notification
permission are still required for actual device delivery.

## Content construction

Ramp registers its Image and Media implementations when linked. Applications use Prism's
`Image::read(reader)`, `Image::from_rgba((width, height), pixels)`, `Media::new()`,
`Media::read(reader)`, and `Media::try_from(...)` with the same arguments as Maverick.
Hardware owns device operations such as picking and capture; it has no content-construction helpers.
`image.pixels()` borrows the decoded RGBA8 buffer and its actual dimensions without copying.
`image.write(writer, ImageFormat::Png)` encodes through the native backend; all eight ImageFormat
variants match Prism and Maverick.

Backend implementers submit `Prism::content_backend::<Image, Media>()` through inventory once.
The returned registration value is opaque; the `ContentBackend` record stays private inside Prism.

## Change futures

Ramp forwards Maverick's `Application::changed().await` to `Prism::changed().await` and
`Application::draw()` to `Prism::draw()`. Air, picker, and playback adapters await their native
handles directly. Task contexts and wakers are confined to Maverick's window-loop adapter.

Front-end components declare separate `#[on_change(source_expression)]` implementations of
`OnChange<Update>`. Prism generates the distinguishing tag and registration, then combines
independently borrowed sources and children into one internal `listen()` future per component.
The winner returns its update and child path. Competing waits are dropped before the root
passes `Context` down that path and calls the matching handler. `Drawable::changed(ctx)`
performs both steps and returns `()`.
Await each handle's `next()` future directly. For optional sources, match the option and await
`std::future::pending()` while it is absent. The next listener rebuild observes the new handle.
