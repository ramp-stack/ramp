# Style Guide

## Scope

- Read and apply this file before editing code. Record coding advice as general rules for future work.
- Consolidate duplicate guidance. Ask about conflicting instructions rather than silently choosing.

## Structure and design

- Order files as imports, aliases/supporting types, data structures, traits, then behavioral structures.
- Keep structures with only data and small helper methods in a section below types and above traits.
- Put public definitions first, then hidden and private definitions in a separate second section.
- Apply the same definition and implementation ordering within both sections.
- Keep each structure followed by its implementations, except those grouped under a trait below.
- Put traits without implementations first in the traits section, before traits with implementations.
- Place all implementations of each trait directly below its definition, including blanket impls.
- Put solutions in structures with clear responsibilities, state, and methods; avoid free functions.
- Treat extra parameters, types, helpers, shared pointers, and abstractions as costly. Prefer direct methods.
- Reuse existing library adapters instead of duplicating them in helper structures; keep cohesive state directly in its owner when possible.
- When many values travel together, model the state they represent instead of passing them separately.
- Design data structures to represent only necessary possibilities; remove optional state when absence no longer has a purpose.
- Extract a helper only when it is reused and names a problem of its own; prefer methods.
- Do not split one operation into wrapper types or a sequence of one-step setup methods.
- Interfaces that only operate on global state should be stateless and should not return an instance or handle.
- Group cohesive behavior in its owning module; reassess boundaries after simplification and consolidate small related modules.
- Migrate dependency layers from the bottom up; place shared code below its consumers to avoid dependency cycles.
- Declare crate dependencies with version requirements rather than local filesystem paths.
- Do not use pub(super), pub(crate), or pub(in ...). Keep implementation fields private and expose deliberate pub APIs between modules.
- Prefer concrete types when implementations are known; add generics only for actual interchangeability.
- Derive standard traits when their field-based behavior is sufficient.
- Prefer project-provided derive macros for structural trait implementations and use supported field representations before extending the macro.
- Put shared trait behavior in blanket implementations, not repeated or generated per-type impls.
- Put generic bounds in angle brackets, not where clauses. Do not declare const values.
- If an owner creates implementations through a trait, include construction in that trait.
- Expose typed platform capabilities in application interfaces; keep native requirements, configuration, and conversions in platform adapters.
- Keep trait APIs minimal; require only essential information and avoid associated types for return types fixed by Self.
- Prefer parameterless marker attributes when behavior uses a fixed method name.
- Keep related API names consistent in prefix and word order.
- Resolve higher-level configuration in the owner and pass only required inputs to its implementations.
- Collection-gathering methods return collections instead of accepting mutable output vectors.
- Combine a fixed set of collections with an array followed by into_iter().flatten().collect().
- Generate smaller tuple arities by recursively reducing one macro invocation, not listing each call.

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
- Do not add helpers, aliases, or temporary variables solely to shorten a clear expression.
- Add code comments only to convey critical information that the code itself cannot clearly express.
- Preserve this style when editing. Default rustfmt with max_width=100 does not enforce these rules.

## Boundaries and state

- Keep runnable examples self-contained in the standard entry point and focused on the public API.
- Avoid extra example files and application setup already provided by the framework.
- Keep layout, rendering, input, and platform responsibilities with their appropriate owners.
- Report hardware actions as application inputs; let the application decide which objects receive those actions.
- Keep application state in its owner. Child controls act on it by calling the owner's methods, through a callback, or by emitting an event. Never share mutable state through Rc/RefCell, Arc or Mutex unless it is tightly contained and easy to reason about.
- Normalize platform units and representations at integration boundaries; keep shared data neutral.
- Follow OS-default device selection and native routing controls; add in-app selectors only when requested.
- Keep platform-specific routing and session handling in native adapters; keep universal behavior shared.
- Gate target-specific code only at module declarations for files named after that target.
- Gate target-specific imports only when they import from those gated modules. Keep target checks out of shared implementations; expose matching platform interfaces instead.
- Apply application volume as per-stream gain with unity as the default, without changing OS volume.
- Platform adapters expose matching, complete interfaces and own native UI, configuration, conversions, and temporary resources; avoid wrappers that split their operations.
- Separate content and validated metadata from live processing, playback, and presentation state.
- Store intrinsic metadata on the content type rather than deriving it from presentation artifacts.
- Let asynchronous handles own pending and completed results; avoid redundant status flags.
- Resource handles own their operation's lifetime; dropping one cancels only its operation and preserves independent queued requests.
- Repeated reads of a completed request return its stored result.
- Queue restricted operations in the layer that owns the restriction.
- Treat resource-result channel disconnection as cancellation; complete once, release request state, and ignore stale callbacks from cancelled operations.
- Preserve successes and per-item failures separately from cancellation; never silently omit failed items.
- Expose errors callers can act on. The framework owns prerequisites, valid inputs, and supported outputs.
- When the framework owns selection UI, handle invalid content and reselection there instead of exposing corruption to application code.
- Handle expected platform conditions internally when this preserves the user experience.
- Treat unexpected platform behavior, broken internal invariants, and unexpected persistent worker-channel closure as fatal bugs.
- Prefer unwrap for simple invariant failures; avoid redundant state changes immediately before panicking.
- Make clone semantics clear: a shared handle shares state; independent instances need new state.
- For live notifications, prefer bounded broadcast; slow or unused subscribers must not block producers or cause unbounded memory growth.
- Unused readers must not cause background panics. Panic only when a caller consumes a lagged subscription; explicit snapshot loading may resubscribe it.

## Validation and reliability

- Define one input format and validate it at the boundary, not repeatedly throughout consumers.
- Do not guard against values the platform cannot produce; raise real input-design problems instead.
- If a public type promises valid content, validate it during fallible construction.
- Keep recovery with the layer that understands the operation and owns the state needed to retry it.
- Tolerate brief recoverable failures but expose sustained gaps; do not silently discard useful data.
- Do not add unrequested fallback behavior or speculative features.
- Avoid dependencies added solely for unrequested alternatives to standard-library types.
- Preserve public behavior and protocol compatibility unless a change is requested.
- Separate raw transport from encryption, application handshakes, request tracking, and retries.
- Establish transport first; perform application handshakes through it, not in its constructor.
- Use shared protocol interfaces at both ends; keep transport selection in startup code.
- Distinguish delivery from durable acceptance. Commit effects and recoverable results atomically.
- Authenticate resumed ownership and prevent superseded connections from continuing to act.
- For rendering and layout changes, verify transparency, clipping, and moving controls across their full range, including endpoints.
- Run necessary checks while working, then remove tests created for the task before finishing.
