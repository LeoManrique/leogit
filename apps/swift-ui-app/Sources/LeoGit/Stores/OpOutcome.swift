import Foundation

/// How a serialized operation ended.
///
/// Three cases and not two, because "git refused" and "we never asked git" are
/// different answers and a caller acts on them differently. Both `BranchStore`
/// and `SyncStore` serialize their work behind a single in-flight slot —
/// branch mutations so two checkouts cannot contend on `index.lock`, transfers
/// because there is one network slot — and both used to answer `String?`, whose
/// `nil` meant *succeeded* and *never ran* alike.
///
/// That collapse is not a theoretical one. It dismissed the create-branch sheet
/// on a branch that was never created, reported a merge that never ran, cleared
/// a delete confirmation while leaving the branch, and — once the force-push
/// confirmation learned to stay open and wait for its answer — would have
/// closed it as though the push had landed, on a background auto-fetch
/// happening to hold the slot at that moment.
///
/// **A refusal is not an error to show.** Nothing went wrong and nothing
/// changed; the surface that asked simply stays as it is, for the user to ask
/// again. That is the case that distinguishes this from an ordinary
/// `Result`: the third state is neither success nor failure, and rendering it
/// as either is exactly the bug.
enum OpOutcome {
    /// The operation ran and git accepted it.
    case succeeded

    /// The slot was held; nothing was attempted and nothing changed.
    case refusedBusy

    /// Git refused. The text is core's own, never re-worded on the way here.
    case failed(String)
}

/// How opening a repository ended.
///
/// The same three-state shape as `OpOutcome` and for the same reason, one
/// question along: a caller cannot act on `RepoStore.repoPath` afterwards to
/// find out, because the answer it would read may belong to a *later* open.
///
/// `superseded` is the case that needs the type. The user asked for another
/// repository while this one was still being read, so this open published
/// nothing and the newer one is authoritative — nothing failed, nothing is
/// stale, and there is no message. A caller that reported it as a failure
/// would raise a banner about a repository the user has already left.
enum OpenOutcome {
    /// This repository is now the open one, whatever its status read returned.
    case opened

    /// The path could not be resolved as a repository. Whatever was open stays
    /// open, and `errorMessage` says why.
    case failed

    /// A later `open` claimed the app while this one was reading.
    case superseded
}
