# Style Guide

## Scope

- Read and apply the current file for every request and after context compaction. Review changed code against it before finishing.
- This is the canonical coding guide and supersedes older AGENT.rs copies.
- Record user-provided coding advice as general rules for future work. Keep project-specific requirements in project documentation and write rules without author prefixes.
- Deploying a crate means bumping its version, pushing the release to GitHub, and publishing it with Cargo.
- Consolidate duplicate guidance. Ask about conflicting instructions rather than silently choosing.

## Structure and design

- Order files as imports, statics, module-level type aliases, data structures, traits, then behavioral structures.
- Order the header as external dependency imports, standard-library imports, imports from elsewhere in the crate, then child file-module declarations (`mod`).
- Place imports and re-exports from each child module directly below its `mod` declaration, not in the preceding crate-import group.
- Place every static immediately below the import/module header, regardless of its visibility. Keep its actual visibility unchanged; prominent placement makes its wider effects easier to see.
- Place every module-level type alias immediately below statics, regardless of its visibility. Keep its visibility unchanged; these definitions are essential to understanding the file and must not be buried in the private section. This placement also applies to aliases used only by one trait implementation.
- Keep structures with only data and simple getters below type aliases and above traits. Structures with behavior beyond getters belong after traits in their public or private section, together with their implementations.
- Except for statics and module-level type aliases, put public definitions first, then private definitions in a separate second section.
- Apply the same definition and implementation ordering within both sections.
- Keep each structure followed by its implementations, except those grouped under a trait below.
- Place each service beside the state and helper types it owns rather than grouping unrelated states separately from their services.
- Place supporting structs and enums used only by one trait implementation directly above the implementing structure. Order them to match the corresponding associated types in that implementation.
- Put traits without implementations first in the traits section, before traits with implementations.
- Place trait implementations directly below the implemented trait's definition. For a blanket implementation of one trait for any type implementing another trait, place it below the bound trait instead; for example, place `impl<T: Source> Target for T` directly below `Source`.
- Order methods in each trait implementation to match their order in the trait definition.
- Never write free-standing functions. Put behavior and constructors in associated methods or trait implementations on their owning types.
- Treat extra parameters, types, helpers, shared pointers, and abstractions as costly. Prefer direct methods.
- Before creating a new file or introducing a new structure, ask an explicit question and receive approval. Explain its purpose and effect on the interface so additions remain understandable; general task approval alone does not approve these additions.
- Reuse existing library adapters instead of duplicating them in helper structures; keep cohesive state directly in its owner when possible.
- Simplify APIs with long argument lists. When many values travel together, model the state they represent instead of passing them separately; discuss exceptional cases before retaining a long parameter list.
- Design data structures and API inputs to represent only supported, necessary possibilities; remove optional state when absence no longer has a purpose.
- Use enums for finite choices instead of strings or other broad input types with unsupported values. Prefer compile-time validation and exhaustive handling whenever the type system can express the constraint.
- Keep configuration specific to an enum variant in that variant, rather than accepting separate parameters that other variants ignore.
- Extract a helper only when it is reused and names a problem of its own; prefer methods.
- Do not split one operation into wrapper types or a sequence of one-step setup methods.
- Prefer direct constructor parameters over traits added only to normalize constructor signatures.
- Interfaces that only operate on global state should be stateless and should not return an instance or handle.
- Group cohesive behavior in its owning module; reassess boundaries after simplification and consolidate small related modules.
- Keep closely related implementations in the same file. Introduce submodules only for substantially different responsibilities or a clearly more compact organization.
- Remove empty source directories left behind when moving or consolidating modules.
- Prefer module-named Rust files such as `host.rs` with children in `host/`; do not use `host/mod.rs` or other `mod.rs` files.
- Declare authored Rust source with `mod`; do not splice source files into another module with `include!`.
- Avoid modules that merely wrap or forward the primary implementation. Introduce modules for substantial, distinct responsibilities.
- Migrate dependency layers from the bottom up; place shared code below its consumers to avoid dependency cycles.
- Keep shared contracts independent of their consumers and platform implementations. Convert layer-specific values at integration boundaries so reusing a contract does not require unrelated application code.
- Use local path dependencies when developing related crates together. Version requirements are not mandatory; use registry or Git dependencies when those sources fit the workflow.
- Remove unused dependencies and disable unused dependency features. Make dependencies used only by a particular feature optional and enable them through that feature.
- Do not use pub(super) or pub(in ...). Allow pub(crate) only on associated `new` constructors for types constructed inside the crate and handed out through public APIs. Keep implementation fields private and expose deliberate pub APIs between modules.
- Do not use `#[doc(hidden)]` to conceal public implementation details. Use actual privacy and module boundaries. If an item cannot be made private, explain which callers need it and why public access is required.
- Prefer concrete types when implementations are known; add generics only for actual interchangeability. Keep input-only generic parameters on constructors or methods when the owner stores a concrete representation.
- Derive standard traits when their field-based behavior is sufficient.
- Express standard operations through their standard traits, such as PartialEq and Eq for equality and Deref for access to an underlying value, instead of equivalent custom methods.
- Prefer public fields for plain data values over constructors and trivial getters. Keep methods for derived values or behavior, and validate raw data where an operation consumes it.
- Expose each piece of data through one canonical API. Avoid overlapping accessors that repeat data already available from a grouped value.
- Prefer project-provided derive macros for structural trait implementations and use supported field representations before extending the macro.
- Put shared trait behavior in blanket implementations, not repeated or generated per-type impls.
- Put generic bounds in angle brackets, not where clauses. Do not declare const values.
- If an owner creates implementations through a trait, include construction in that trait.
- Expose typed platform capabilities in application interfaces; keep native requirements, configuration, and conversions in platform adapters.
- Keep trait APIs minimal; require only essential information and avoid associated types for return types fixed by Self.
- Prefer parameterless marker attributes when behavior uses a fixed method name.
- Keep related API names consistent in prefix and word order.
- Name command-line interface structures `Cli` and refer to them through their owning module paths.
- Keep corresponding frontend and backend APIs aligned in ownership, names, parameters, and behavior. Ask for explicit approval before introducing a difference.
- Resolve higher-level configuration in the owner and pass only required inputs to its implementations.
- Collection-gathering methods return collections instead of accepting mutable output vectors.
- Generate related macro variants from one recursive declaration instead of manually listing repetitive invocations.

## Formatting

- Aim for roughly 100 characters per line. Keep naturally short lines short.
- Function signatures may extend to 150 characters; keep other code around 100 characters.
- Keep a fitting signature, including its return type, on one line; measure it without the body.
- Never split a fitting signature to keep its body inline. Put the body below instead.
- Keep a complete short method inline when it fits around 100 characters.
- Wrap only overlong signatures: name and "(" first, grouped parameters next, then ")" and return type.
- Split long parameter lists into balanced groups, not one parameter per line.
- End a wrapped signature with "{"; put its body below and its closing "}" on a separate line.
- When a body or collection must span lines, end its opening line with "{" or "[".
- Put the contents on the following line(s), then the closing "}" or "]" on its own line.
- A closing delimiter may retain required punctuation or a following method chain.
- Keep small declarations, literals, branches, closures, and loops inline when they fit.
- Put each field in a named-field struct definition on its own line; never combine fields to save space.
- Group related values and chain operations; do not expand every element onto its own line.
- Use blank lines for logical groups, not between every short method.
- Use concise, descriptive named types to resolve type-complexity warnings. Otherwise, do not add helpers, aliases, or temporary variables solely to shorten a clear expression.
- Add code comments only to convey critical information that the code itself cannot clearly express.
- Preserve this style when editing. Default rustfmt with max_width=100 does not enforce these rules.

## Boundaries and state

- Keep demos in the library crate's `src/main.rs`, self-contained and focused on the public API.
- Do not create separate demo projects or extra example files. Reuse application setup provided by the framework.
- Keep layout, rendering, input, and platform responsibilities with their appropriate owners.
- In CLI tools, keep authored user-visible wording in `cli.rs` files. Working logic propagates structured errors and context upward; the CLI chooses how to describe them and drives the user interaction.
- Present CLI setup as short, clearly separated steps with descriptive prompts. Display clickable links to setup pages instead of opening them automatically, and keep redirected output readable as plain text.
- Keep framework-wide coordination queries internal rather than exposing them on public resource handles.
- Make controls interactive throughout their intended layout area, including assigned padding, independently of the size of their visible content.
- Use statics and global OnceLock bindings only as a last resort. Prefer context-owned, clonable capability handles and pass dependencies explicitly.
- Drive optional behavior through the existing shared instance and its normal service; avoid parallel clients, registries, or per-consumer workers for the same state.
- Activate controls on touch/press rather than release unless the interaction requires otherwise. Preserve activation timing when adding gesture handling.
- Let container drag gestures begin in unused interior space as well as their handles; give child controls and scrolling priority for the entire gesture when they use its initial press.
- Report hardware actions as application inputs; let the application decide which objects receive those actions.
- Let child controls emit events to their owner; keep application state and action handling in the owner instead of sharing mutable state with controls.
- Keep background services independent of UI contexts and general hardware interfaces; pass only the capabilities their work needs.
- Normalize platform units and representations at integration boundaries; keep shared data neutral.
- Scope credentials and mutable state to their owner; shared services must isolate unrelated owners.
- Follow OS-default device selection and native routing controls; add in-app selectors only when requested.
- Keep platform-specific routing and session handling in native adapters; keep universal behavior shared.
- Gate target-specific code only at module declarations for files named after that target.
- Gate target-specific imports only when they import from those gated modules. Keep target checks out of shared implementations; expose matching platform interfaces instead.
- Keep application-specific adjustments local to the resources they control, use neutral defaults, and leave system-wide settings unchanged unless explicitly requested.
- Platform adapters expose matching, complete interfaces and own native UI, configuration, conversions, and temporary resources; avoid wrappers that split their operations.
- Expose capabilities through platform adapters while keeping native implementation types private. Do not export implementation aliases solely to make shared code compile across platforms.
- Keep content and its validated metadata separate from processing and presentation state.
- Store intrinsic metadata on the content type rather than deriving it from presentation artifacts.
- Let asynchronous handles own pending and completed results; avoid redundant status flags.
- Await asynchronous completion at the layer that owns the operation, and keep retries there instead of requiring consumers to poll readiness.
- Prefer ordinary async/await, standard future combinators, and library synchronization primitives over custom polling methods, task contexts, waker plumbing, and atomic flags with manual memory ordering. Keep unavoidable low-level scheduling or synchronization inside a small runtime or platform adapter rather than exposing it through application APIs.
- Keep current-state retrieval separate from waiting for changes. In snapshot subscriptions, loading current state establishes the observation baseline and discards older unread updates; the next wait returns an update newer than that snapshot. Preserve the existing shared-state subscription contract when wrapping it.
- A handle that owns an operation also owns its lifetime; dropping it cancels only that operation and preserves independent operations.
- Repeated reads of a completed request return its stored result.
- Queue restricted operations in the layer that owns the restriction.
- Distinguish cancellation from failures according to the operation's ownership contract. Complete each operation once, release its state, and ignore stale callbacks after cancellation.
- Preserve successes and per-item failures separately from cancellation; never silently omit failed items.
- Expose errors callers can act on. The framework owns prerequisites, valid inputs, and supported outputs.
- Validate user-selected content in the layer that owns the selection workflow, and keep correction or reselection there before passing valid data to consumers.
- Handle expected platform conditions internally when this preserves the user experience.
- Keep prerequisites, permission checks, and expected best-effort failures in the layer that owns the operation; callers should express the action they need.
- Make clone semantics clear: a shared handle shares state; independent instances need new state.
- Cache lazy initialization in the shared state so repeated accesses and cloned handles reuse the same initialized value.
- For updates delivered to independent consumers, prefer bounded fan-out; slow or unused consumers must not block producers or cause unbounded memory growth.
- Idle or unused consumers must not crash background services. Classify failures when data is consumed using the recoverable-error and invariant rules below.

## Validation and reliability

- Define one input format and validate external data at the boundary, converting it to supported typed values before passing it to internal APIs. Keep unavoidable runtime parsing at that boundary rather than accepting unvalidated values throughout consumers.
- Do not guard against values the platform cannot produce; raise real input-design problems instead.
- Use recoverable errors only for realistically expected failures that callers can recover from. Panic on violated invariants, programming mistakes, and states so unlikely in intended use that they indicate a bug.
- Do not make APIs fallible or add recovery paths solely for theoretical limits outside realistic use.
- Prefer unwrap for simple invariant failures; avoid redundant state changes immediately before panicking.
- If a public type promises valid content, validate it during fallible construction.
- Keep recovery with the layer that understands the operation and owns the state needed to retry it.
- Recover expected transient failures inside their owner using the simplest suitable mechanism. Panic when recovery is exhausted and the caller or user has no useful action, including when required persistent storage cannot be opened; do not invent recovery UI or replacement data.
- Tolerate brief recoverable failures but expose sustained gaps; do not silently discard useful data.
- Do not add unrequested fallback behavior or speculative features.
- Ask for and receive explicit user approval before adding backward-compatibility or migration code. For configuration changes, prefer requesting the required values again over retaining legacy formats.
- Avoid dependencies added solely for unrequested alternatives to standard-library types.
- Preserve public behavior and protocol compatibility unless a change is requested.
- Keep lower-level resource establishment separate from higher-level protocols, request tracking, and recovery. Establish the resource first, then perform application-level negotiation through it.
- Share protocol contracts between communicating components and choose concrete transports at the composition boundary.
- Distinguish receipt from durable completion. Commit state changes and the results needed for recovery atomically.
- Validate ownership when resuming an operation and prevent superseded owners from continuing to act.
- For rendering and layout changes, verify transparency, clipping, and moving controls across their full range, including endpoints.
- Always run `cargo clippy` for affected Rust crates before finishing code changes, covering the relevant targets and features. Address every warning; if a recommendation conflicts with this guide, ask before applying it or suppressing the warning.
- Run necessary checks while working, then remove tests created for the task before finishing.
- Run task virtual machines, simulators, and emulators in the background or on an unused desktop; do not cover the user's active workspace or take focus.
- Shut down virtual machines, simulators, and emulators started for a task before finishing, unless explicitly asked to leave them running. Preserve instances that were already running before the task.
