# RAMP

RAMP connects Maverick OS hardware, AIR services, and Prism's application interfaces.

## Building

The shared `media2` crate, imported as `media`, requires `ramp-ffmpeg` 9.0.4 or later
and `ramp-ffmpeg-sys` 9.0.5 or later. FFmpeg is linked statically; no installed FFmpeg
package or runtime is needed.

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

## Native text

RAMP mirrors Prism text in transparent native controls on iOS, Android, and macOS.
Drawing instructions supply identity, text, font, focus, position, and clipping on every frame.
Prism continues to render the text; native controls provide selection and clipboard menus.
Other desktop targets currently leave text interaction entirely to Prism.
On iOS, tapping an empty line in read-only text selects its newline. Holding and releasing keeps
that selection and opens the native copy menu. Word selection and selection handles remain native.
Read-only text excludes AutoFill from the native menu; editable fields retain their normal menu.
Unchanged read-only text keeps its native contents and selection while its overlay follows the
latest drawing position. Editable mirrors continue reconciling native edits on every update.

Ordinary typing follows Prism's keyboard events. Native selection is converted to UTF-8
offsets, and clipboard edits return through RAMP's private, ID-targeted `SetText` event.
Pelican text editors observe those edits and emit their normal change events. Native
callbacks are processed by input handling or `changed()`, never by `draw()`.

## AIR state

RAMP restores its root secret from `ramp.air.identity`. Valid saved secrets are reused without
rewriting. A missing, non-text, corrupt, non-root, or mismatched secret creates a new root identity.
RAMP tries to save that replacement and logs a warning if persistence fails, continuing with the
new identity for the current session.

RAMP stores one shared `air::Air` per account and drives its catalog and inbox requests
through the normal AIR service. `Instances<C>` remains the collection and event source for
one contract type. Creation goes through `air::Air::create`, and `Instance<C>::share`
forwards to `air::Air::share`.

The bridge saves a newly initialized instance, then saves the catalog and publishes its location.
Existing initialized instance caches are reused. The new account state uses a fresh `AirState`
cache; the previous `InboxState` cache is not migrated. Server-synchronized locations are
rediscovered, but unsent work in the old inbox cache is not recovered.

## Notifications

Enable notifications directly on an AIR instance:

```rust
instance.notify(true);
instance.notify(false);
```

Notifications default to false for new instances. The flag is shared by all clones of
that instance and is independent of other instances. Calling `notify(false)` removes its remote
watch. Each instance keeps an `air_notifications::Subscription` that decides when to alert,
watch, acknowledge, or remove a watch. The enabled flag is saved with the instance cache and
restored on reopening; catch-up state starts fresh so replay stays quiet. The flag is local to
the installation and is not saved in the contract. The normal instance service executes those actions;
there is no separate application notification client or per-instance notification service.

Instance caches use the `ramp2::air::InstanceState` namespace with the saved notification flag.
The previous `ramp2::air::instances::InstanceState` caches are not read or migrated. Instances
rebuild from their server history; unsent work in previous instance caches is not recovered.

Only live incoming updates after catching up to the channel head produce local alerts. Historical
replay and confirmation of the current user's own writes do not alert. In the foreground, Maverick
sends `Input::Notification(header, body)`, which RAMP broadcasts as `prism::events::Notification`.
In the background, Maverick posts a native device notification. The built-in alert is generic;
Maverick's `hardware.notifier()` returns a clonable `Notify` handle for custom local content through
`notify.notify(header, body)`. Services receive that handle during setup and do not retain Hardware.

RAMP holds one shared notification contract per AIR identity, installation, and application
configuration through its own `air::Instance`, using the same instance and account services as other
contracts. Its only additional worker observes the hardware push destination and updates that
contract on token rotation. It owns Maverick's `PushDestination` handle and awaits `next()` directly; native
registration and token waiting stay in Maverick. The token and signing credentials stay out of Prism.

At each channel head, RAMP registers the next public file key with the notification server.
The watch remains armed while the app is online. After handling an update, RAMP acknowledges
that file. The server delays an available file's alert by 500 ms and cancels it if the owner's
acknowledgment arrives first. Late acknowledgments cannot remove a newer watch. A delayed or
lost acknowledgment can still produce a duplicate device alert; the grace period is best effort.
App suspension needs no background WebSocket guarantee: an armed remote watch survives it.

### Application configuration

The `air_notifications` crate owns the notification contracts, its `PushDestination`, and the
standalone server. It has no dependency on Maverick, Prism, or RAMP. RAMP re-exports its contracts
as `ramp2::notifications` and converts Maverick's `PushToken` in the existing push service.
The contract retains both the provider and token so its server can deliver through APNs or FCM.
The crate also owns notification policy and filters redundant contract messages through
`Notifications::changes`. RAMP supplies the existing AIR instances and native notification handles.
RAMP's `src/notifications.rs` contains registration setup, native action delivery, and the `Push`
service. Its public `Push` bridge is called by `air.rs`; the service's fields remain private.
The existing `air::Instance` is public so this sibling module can create and use the backend handle
directly, without constructing Prism's `Air` or `Instance` wrappers.

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

Development uses sibling paths for the frontend crates, Maverick, and AIR. RAMP and Orange
patch registry AIR dependencies to the same local checkout so the transport, notification,
and frontend crates use the same contract types. `.cargo/config.toml` retains frontend overrides.
Apple push entitlements, signing, and notification permission are still required for actual delivery.

## Content construction

Prism, Maverick, and Ramp depend directly on `media2`, imported as `media`. Image and
media decoding, editing, and encoding use the same types throughout the stack. Ramp has no
Image or Media codec bridge or content-backend registration.

`Image` has public `dimensions` and dereferences to `Vec<Pixel>`. `Image::new(dimensions, pixels)`
takes an owned pixel buffer, and `Image::read(reader)` decodes a still image. Clones share pixels
until mutation requires a private copy. `image.as_ref()` borrows its RGBA8 bytes, and
`image.write(writer, ImageFormat::Png)` encodes it. Prism displays `Image` directly; cloning a view
shares its pixel allocation. SVGs remain separate drawables.

`Media::read(reader)` retains an owned, seekable reader for lazy decoding. Use a file or
`std::io::Cursor<Arc<[u8]>>` for shared in-memory bytes. `Audio` and `Video` are independent
tracks; `Media` combines them. `Visual::Video(Media)` picker results retain the soundtrack,
while a typed `Video` selection returns only the visual track.

Create native playback through `ctx.hardware.player(media.player())`. Ramp's shared `Player`
owns the core player and schedules updates, publishing snapshots through Maverick's Shared
system. The core `media::Player` owns the clock, seeking, loop/live modes, decoding, and output
samples; Maverick's `Speaker` owns the device queue and native audio session. Ramp connects
these capabilities and reports playback state through Maverick's native `Playback` adapter.
`frame()` establishes this handle's snapshot baseline; `next().await` returns the next update
after it. Clones share playback while retaining independent subscriptions, and dropping the
last handle stops its service and output. Displayed video retains its `Arc<VideoFrame>`
without copying pixels. Camera and microphone handles yield raw timestamped frames through
`next().await`; applications choose whether to preview them, insert them into a timeline, or both.

## Change futures

Ramp forwards Maverick's `Application::changed().await` to `Prism::changed().await` and
`Application::draw()` to `Prism::draw()`. Air and picker adapters await their native handles;
playback awaits Ramp's shared frame subscription. Task contexts and wakers are confined to
Maverick's window-loop adapter.

Front-end components declare separate `#[on_change(source_expression)]` implementations of
`OnChange<Update>`. Prism generates the distinguishing tag and registration, then combines
independently borrowed sources and children into one internal `listen()` future per component.
The winner returns an owned callback accepting the component and `Context`. Competing waits
are dropped before the root invokes it. Generated callbacks forward directly to the selected
field or collection entry; there is no path vector or separate update-dispatch method.
`Drawable::changed()` returns that callback; `Prism::changed()` awaits and invokes it with
the root drawable and `&mut Context`, then returns `()`.
Await each handle's `next()` future directly. For optional sources, match the option and await
`std::future::pending()` while it is absent. The next listener rebuild observes the new handle.

Ramp enables `wgpu_prism`’s `renderer` feature. Prism requests only its `drawable` feature,
which provides the shared drawing primitives without WGPU.
