# LeoGit Instructions for Claude Code

## Implementation Log

Constantly update these documents without repeating information between them.

- Update `./STYLE.md` after significantly changing the frontend style. Remove obsolete information.
- Update `./DESIGN.md` after completing any functional design feature or big change. Remove obsolete information.
- Update `./TECHNICAL.md` after changing any technical architecture or implementation detail. Remove obsolete information.
- Update `./ROADMAP.md` after completing any point mentioned there. Also add new features that come up while developing.
- Update `./README.md` after completing some change that affects the escense of the project. Keep it concise and remove obsolete information.

"Remove obsolete information" means rewrite: these documents describe the current
state only, as if older behavior never existed. Never append retirement or
supersede notes ("since superseded", "formerly", "was removed", "used to"), and
never leave a stale claim standing next to its correction — delete the claim and
say only what is true now. When an obsolete passage carried a constraint worth
keeping, restate the constraint in present tense ("deliberately does not X,
because…"). History and decision records belong in git and in `docs/plans/`
as-built records — those keep their amendment notes; the living docs do not.
ROADMAP's checked entries log what each completed chunk did at the time and may
be contradicted by later entries; that is fine — but never patch one with a note.

## Implementation order

Don't implement too many multiple features at once, make the feature implementation order what the user flow is, and make sure it is tested before proceeding to the next one.

## Long-term correctness over immediate fixes

- Our objectives are now not about fixing bugs, instead we need to focus on making the app as solid as possible. When deciding something try to take the approach that involves a long-term correctness.

## Clean, Mantainable, and Readable Code

Follow DRY and Single Responsability principles to make our code clean, mantainable, and readable code. Try re using components that are used by multiple features when possible. Also give files and folders a meaningful name.

Run clippy pedantic to see if rust recommendations are being followed.

## Always fix

Do not ignore the errors or warnings when building or running the projects, even if they were already present or not caused by your changes. Do not bypass errors or warnings, actually fix them.

## Debug mode

For key features in both the frontend and backend, keep meaninful logs that will be useful in case of errors. Dont delete the most important ones until we make it to production, the platform is still under development.

## Prefer newer versions

If we have to decide between downgrading or updating versions of packages or libraries because of an issue, always choose upgrading. Never downgrade them to fix an issue.

## Visual testing

Do not use screenshots for testing, ask to visually check the result of a change and wait for confirmation to consider it as complete.

## Always fetch Swift UI current docs before writing code

Before making non-trivial changes in this repo to Swift UI app, pull current documentation via the **context7** MCP server or `WebFetch` against developer.apple.com. Built-in knowledge lags Apple's SDK churn.

- **Latest Swift version**. Resolve `swift` on context7 and fetch docs for whatever API you're touching.
- **Latest SwiftUI / AppKit / Darwin libproc** (latest macOS SDK). Always:
  1. Identify the precise type/function you're using
  2. Fetch its current docs before writing code that depends on its signature or behavior.

Skip the lookup only for trivial edits (renames, comment-only changes, etc.).

Suggest to upgrade libraries to user when available. Wait for their approval to proceed with the upgrade.

## English tutor

The user is not a native English speaker. When you receive a prompt in English, correct the user if they:
- Gave an instruction whose meaning you could infer, but which was incorrect or grammatically awkward.
- Used an incorrect grammar structure.
- Made common English mistakes.

Don't correct them or mention "mistakes" if:
- They used slang or errors that native speakers also make.
- The mistake they made was very likely a typo.

Put the correction at the end of your response, below a `---` separator, showing the corrected version and a one-line explanation.
Avoid completely changing the user's writing style, and don't use em dashes or an AI-sounding writing style.
Don't mention anything if there are no mistakes.
