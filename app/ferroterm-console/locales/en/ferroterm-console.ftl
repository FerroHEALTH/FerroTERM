# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1

# The English catalog of the console's own words: the reference catalog and
# the one every lookup falls back to. Each message is one `Msg` of the
# crate's `locale` module; a catalog of another language holds the same
# messages in a directory named by its BCP 47 tag beside this one. The words

## The marks FerroUI's number formatter writes numbers with

number-group = ,
number-decimal = .

## The document

description = Browse and edit the code systems, value sets and concept maps a FerroTERM server serves.

## The screen an address that names no page reaches

may-edit-conceptmap = May edit ConceptMap
may-edit-valueset = May edit ValueSet
may-edit-codesystem = May edit CodeSystem
may-edit-valueset-conceptmap = May edit ValueSet and ConceptMap
may-edit-codesystem-conceptmap = May edit CodeSystem and ConceptMap
may-edit-codesystem-valueset = May edit CodeSystem and ValueSet
may-edit-codesystem-valueset-conceptmap = May edit CodeSystem, ValueSet and ConceptMap
read-only-account-has-no-editing = Read only: this account has no editing permission

every-group-carries-least-one-code = Every group carries at least one code, which is what a group is for.
every-group-names-system-maps-from = Every group names the system it maps from and the system it maps to.
concept-map-needs-publication-status = A concept map needs a publication status.
concept-map-needs-canonical-before-can = A concept map needs a canonical before it can be saved.
n-1-element-differs = 1 element differs.
these-two-versions-state-same-elements = These two versions state the same elements.
one-these-two-versions-carries-no = One of these two versions carries no resource, so there is nothing to compare.
choose-two-versions-above-compare-them = Choose two versions above to compare them.
server-s-capability-statement-declares-neither = This server's capability statement declares neither the history interaction nor the version read for this resource type, so it offers no versions to read.
server-holds-no-earlier-version-resource = This server holds no earlier version of this resource.
what-root-says-serves = What the root says it serves
software = Software
not-served = not served
answer-did-not-read = The answer did not read
request-got-no-answer = The request got no answer
server-refused-request = The server refused the request

description-2 = Description
last-changed = Last changed
publisher = Publisher
experimental = Experimental
status = Status
version = Version
title = Title
name = Name
and = , and{" "}
clause-takes-back-out-every-code = This clause takes back out every code that
clause-draws-every-code = This clause draws in every code that
comes-from-code-system-named-here = comes from the code system named here
value-set-named-here = is in the value set named here
parameters = Parameters
clause-lists-codes-filters-once-split = This clause lists codes and filters at once. Split it into two clauses: they union.
clause-lists-codes-filters-which-name = This clause lists codes or filters, which name a code system to be read from.
clause-names-neither-value-set-nor = This clause names neither a value set nor a code system, so it selects nothing.
groups = Groups
translates-to = Translates to
translates-from = Translates from
purpose = Purpose
address-not-below-server-s-root = the address is not below the server's root
server-sent-no-diagnostic = the server sent no diagnostic
locked-to-date = Locked to date
inactive-codes-selected = Inactive codes selected
immutable = Immutable
sends-phrase-filter-parameter-code-system = Sends the phrase as the filter parameter, in the code system you pick there.
searches-concept-browser-code-system-you = Searches the concept browser for it, in the code system you pick there.
opens-runner-canonical-filled-ready-code = Opens the runner with the canonical filled in, ready for a code.
validate-code = Validate a code in it
sends-canonical-url-parameter-valueset-expand = Sends the canonical as the url parameter of ValueSet/$expand.
expand-value-set = Expand it as a value set
walks-hierarchy-where-served-version-declares = Walks the hierarchy, where the served version declares an operator to walk it by.
browse-its-concepts = Browse its concepts
reads-what-root-declares-about-codesystem = Reads what this root declares about it, and the CodeSystem it publishes for it.
open-the-code-system = Open the code system
a-phrase = a phrase
a-code = a code
a-canonical = a canonical
version-declares-no-hierarchy-filter-operator = This version declares no hierarchy filter operator, so its concepts are a flat list and there is no tree to walk.
root-s-terminology-capabilities-do-not = This root's terminology capabilities do not name this code system. Another FHIR version may serve it, so try the version switcher above; otherwise this deployment has not loaded it.

content-mode = Content mode

root-declares-none-operations-would-apply = This root declares none of the operations that would apply to what you typed.
where-it-goes = Where it can go
nothing-was-typed = Nothing was typed.
version-carries-no-resource-put-back = This version carries no resource to put back.

back-concept-map = Back to the concept map
back-value-set = Back to the value set
back-code-system = Back to the code system
target-fhir-version-requires-comment-target = The target `{ $code }` of `{ $element }` is `{ $relationship }`, and this FHIR version requires a comment on a target that says so.
target-states-no-relationship-source-which = The target `{ $code }` of `{ $element }` states no relationship to its source, which every served version makes mandatory.
marked-mapping-nothing-carries-target-well = `{ $element }` is marked as mapping to nothing and carries a target as well. Untick that mark to see the target, or clear the mark.
elements-differ = { $more } elements differ.
of-roots-answered = { $answered } of { $answers } roots answered

every-one-value-sets-named-here = is in every one of the { $several } value sets named here
one-codes-listed = is one of the { $concepts } codes listed
matches-every-one-filters = matches every one of the { $filters } filters
nothing-selects-yet = { $verb } nothing selects yet.
server-answered-status = the server answered a status of { $status }
same-concept = { $code_a } and { $code_b } are the same concept.
neither-nor-subsumes-other = Neither { $code_a } nor { $code_b } subsumes the other.
server-answered-outcome-which-not-one = The server answered the outcome `{ $other }`, which is not one of the four this console has wording for.
w = W/"{ $version_id }"

version-declares-operator-filter-so-tree = This version declares the { $child_of_operator } operator on the { $property } filter, so the tree below walks one level at a time.
version-declares-hierarchy-operators-none-them = This version declares the hierarchy operators { $operators }, and none of them selects the direct children of a code, so the concepts read as a flat list.

state-answered = answered
state-refused-word = refused
state-declared = declared
noun-code-systems =
    { $count ->
        [one] code system
       *[other] code systems
    }
    .empty = No code systems
    .opening = Code systems
noun-concept-maps =
    { $count ->
        [one] concept map
       *[other] concept maps
    }
    .empty = No concept maps
    .opening = Concept maps
noun-value-sets =
    { $count ->
        [one] value set
       *[other] value sets
    }
    .empty = No value sets
    .opening = Value sets

subsumes-descendant = { $code_a } subsumes { $code_b }: { $code_b } is a descendant of { $code_a }.
subsumed-descendant = { $code_a } is subsumed by { $code_b }: { $code_a } is a descendant of { $code_b }.
title-find = Find

## The chrome around every page

shell-console = Console
shell-navigation = Console
shell-group-overview = Overview
shell-group-terminology = Terminology
shell-group-operations = Operations
shell-group-console = Console
shell-dashboard = Dashboard
shell-code-systems = Code systems
shell-value-sets = Value sets
shell-concept-maps = Concept maps
shell-concept-browser = Concept browser
shell-operations = Operations
shell-server = Server
shell-configuration = Configuration
shell-about = About
shell-search-label = Find in the terminology server
shell-search-placeholder = Resource, code or phrase
shell-fhir-version = FHIR version
shell-server-up = Server up
shell-server-degraded = Server degraded
shell-server-refused = Server refused 401
shell-server-unreachable = Server unreachable
shell-server-at = at { $root }
shell-sign-in = Sign in
shell-signed-in = Signed in
shell-not-signed-in = Not signed in
shell-visitor-menu = Visitor menu
shell-visitor-line = Reading is open; Sign in is for editing.
shell-preferences-menu = Your preferences
shell-open-line = This console has no sign-in.
shell-development-title = Development deployment
shell-development-body = this console has no sign-in, and the server it reaches has authentication switched off. Anyone who reaches it can change the stored resources.

## Where a resource comes from, and its versions in words

source-stored = Stored on this server
source-stored-tip = Written through the FHIR API and kept in the server's resource store; it can be edited and has a history
source-index = Offline index
source-index-tip = Served from an index built offline; read-only
source-loaded = Loaded at start
source-loaded-tip = Read from a resource directory, or carried by the server, when it starts; read-only
versions-none = none declared
versions-one = Version { $version }
versions-many = Versions { $versions }
type-code-system = Code system
type-value-set = Value set
type-concept-map = Concept map

## How long ago a moment was

ago-seconds = { $count } s ago
ago-minutes = { $count } min ago
ago-hours = { $count } h ago
ago-yesterday = yesterday
ago-days = { $count } days ago

## The dashboard

dashboard-subtitle = What { $host } serves on FHIR { $version }, as it answered at { $time }.
dashboard-subtitle-unanswered = What { $host } serves on FHIR { $version }.
dashboard-serves-nothing = FHIR { $version } serves no code systems, value sets or concept maps on this server.
dashboard-read-instead = Read FHIR { $version } instead
dashboard-stored = { $count } stored on this server
dashboard-since = since { $day }
dashboard-recent-writes = Recent writes
dashboard-recent-changes = Recent changes
dashboard-resource = Resource
dashboard-change = Change
dashboard-when = When
dashboard-type-version = { $type } · version { $version }
dashboard-created = Created
dashboard-updated = Updated
dashboard-deleted = Deleted
dashboard-nothing-written = Nothing written yet: every resource on this root was loaded when the server started.
dashboard-code-system = Code system
dashboard-source = Source
dashboard-versions = Versions
dashboard-all-code-systems = All code systems
dashboard-failure = Failure
dashboard-address = Address
dashboard-try-again = Try again
dashboard-keeps-no-copy = The console keeps no copy of the server, so nothing shows until it answers; your session is kept.
dashboard-technical-detail = Technical detail

## Your preferences

preferences-title = Your preferences
preferences-save = Save
preferences-accent = Teal
preferences-default-fhir = Default FHIR version
preferences-default-fhir-line = Used when an address names no version. The version select in the top bar switches only this session.
preferences-display-language = Display language
preferences-display-language-line = Language for concept names, for example nl-NL. Empty takes the server's own choice.
preferences-display-language-example = for example nl-NL
preferences-page-size = Page size
preferences-page-size-line = How many rows a paged screen asks for, from 1 to 1,000.

## About

about-title = About
about-subtitle = The console, the server it reads, and the manufacturer.
about-server = Server
about-server-unanswered = The server did not answer
about-fhir-roots = FHIR roots
about-ferroui = FerroUI
about-licence = Business Source License 1.1
about-book = The FerroTERM book
about-statement-title = The CapabilityStatement of FHIR { $version }
about-statement-line = What the { $version } root declares at GET { $path }; Server compares the four roots.

## Server

server-subtitle = What each FHIR root of { $host } declares, and the evidence this build ships.
server-sections = Server sections
server-tab-versions = FHIR versions
server-tab-operations = Operations
server-tab-evidence = Evidence
server-no-answer = no answer
server-roots-silent = { $roots } did not answer, so its column says so.
server-retry = Retry
server-own-operation = FerroTERM's own
server-operations-title = Operations each root declares
server-operations-intro = Which operations each root declares, and at which level.
server-versions-title = What each root declares
server-base-address = Base address
server-code-systems = Code systems
server-declaration = Declaration
server-evidence-line = Read from the files the FerroTERM { $release } build commits, not from the server answering this page.
server-mode = Mode
server-surface = Surface
server-passed = Passed
server-needs = Content it needs
server-needs-nothing = No extra content
server-passed-of = { $passed } of { $ran }
server-suite-title = The HL7 terminology ecosystem suite
server-suite-intro = From the committed pass lists; only general runs in CI.
server-benchmark = Benchmark
server-target = Target
server-measured = Measured
server-microseconds = { $value } µs
server-latency-title = Latency
server-latency-intro = Each target is the project's claim; the measurement is one run on { $machine }.

## Configuration

config-subtitle = The settings in { $file }, read when the console starts.
config-sections = Configuration sections
config-how = How configuration works
config-how-read = The console reads its settings once, when it starts: from { $file }, then from the FERROTERM_CONSOLE__ environment variables, which win over the file.
config-how-change = To change a setting, edit the file or set the variable and restart the console. A secret can also be read from a file through its _file key, so it never sits in the configuration itself.
config-how-shown = Each tab shows one section as it runs, with every secret hidden. The book's console page lists every key.
config-more = More configuration actions
config-copy-path = Copy the file's path
config-connection = Connection
config-sign-in = Sign-in
config-sessions = Sessions
config-listener = Listener
config-server = Server
config-server-line = Where the console reaches the terminology server, on your private network.
config-public = Public address
config-public-line = The address readers see the FHIR API at, shown on the screens.
config-sync = Synchronisation service
config-sync-line = The synchronisation service's admin listener, kept for the screens of its runs.
config-timeout = Timeout
config-timeout-line = How long one call to the server may take.
config-not-set = not set
config-set-up = Set up
config-issuer = Identity provider
config-issuer-line = Where readers sign in to edit, by OpenID Connect.
config-client = Client
config-client-line = The console's client at the provider.
config-secret = Client secret
config-secret-line = Read from a file; never shown.
config-redirect = Redirect address
config-redirect-line = Where the provider sends a reader back.
config-scopes = Scopes
config-scopes-line = What the console asks the provider for.
config-sign-out = Sign-out
config-sign-out-line = Ends the session at the provider too.
config-secure-cookie = Secure cookie
config-secure-cookie-line = The session cookie is sent over HTTPS only.
config-idle = Idle timeout
config-idle-line = A session ends after this long without a request.
config-absolute = Absolute timeout
config-absolute-line = A session ends after this long, even while in use.
config-limit = Session limit
config-limit-line = How many sessions the console holds at once.
config-listen = Listener
config-listen-line = Address and port the console listens on.
config-site-root = Site root
config-site-root-line = Where the console's built site is read from.
config-no-sign-in = Sign-in is not set up
config-no-sign-in-line = Reading stays open; editing needs an [oidc] section.
config-on = On
config-off = Off
config-seconds = { $count } s
config-minutes = { $count } min
config-hours = { $count } h
config-milliseconds = { $count } ms

## Sign in

sign-in-title = Sign in to edit
sign-in-read-only-title = Read-only console
sign-in-at = Sign in at
sign-in-taking = Taking you to
sign-in-read = Read without signing in
sign-in-read-line = : every screen is open to read.
sign-in-note = You sign in at your organisation's identity provider; the console never sees your password and the browser never holds a token.
sign-in-documentation = Documentation
sign-in-privacy = Privacy notice
sign-in-caption = The console of the FerroTERM terminology server. It holds no patient data.
sign-in-try-again = Try again
sign-in-signed-out = You are signed out of the console and of the identity provider.
sign-in-expired = Your session ended after a while without a request. Sign in again to edit.
sign-in-expired-absolute = Your session ended: a session lasts a fixed time at most, even while in use. Sign in again to edit.
sign-in-full = The console has no free sessions right now. Try again in a few minutes.
sign-in-refused = The identity provider refused the sign-in. Try again; if it refuses again, ask whoever runs it.
sign-in-rejected = The sign-in could not be completed: the provider's answer did not pass the console's checks, so nothing was kept. Ask whoever runs the identity provider to check the console's registration.
sign-in-unreachable = The console could not reach the identity provider. Try again in a moment; it may be down.
sign-in-unavailable = This console has no sign-in configured. Every screen is open to read.
sign-in-unavailable-detail = No [oidc] section.

## The error pages

errors-not-found-title = No such screen
errors-not-found-body = The console has no page at { $path }. The navigation lists every screen.
errors-forbidden-body = You are signed in as { $who }, and this account may not do this: { $what }. You can still read everything and preview what you compose.
errors-forbidden-scope = Needs the scope
errors-sign-in-again = Sign in again
errors-unread-title = The page did not load
errors-unread-body = The console could not read this page from its server: { $reported }. Load the page again, or go back to the dashboard.

## The keyboard shortcuts

keys-title = Keyboard shortcuts
keys-intro = A shortcut acts only when no field has the focus. Every shortcut has a control on the screen that does the same.
keys-or = or
keys-everywhere = Everywhere
keys-find = Find a resource, a code or a phrase
keys-legend = This legend
keys-close = Close a dialog, a menu or a panel
keys-dashboard = Go to the dashboard
keys-code-systems = Go to code systems
keys-value-sets = Go to value sets
keys-concept-maps = Go to concept maps
keys-browser = Go to the concept browser
keys-operations = Go to operations
keys-server = Go to the server
keys-tree = Concept tree
keys-tree-move = Previous and next concept
keys-tree-open = Open a level, or go to its first child
keys-tree-close = Close a level, or go to the parent
keys-tree-read = Read the concept
keys-operations-group = Operations and editors
keys-run = Run, or run the preview
keys-save = Save

## The listings

listing-find-chip = Name: { $words }
listing-source-chip = Source: { $source }
listing-source = Source
listing-source-show = Show sources
listing-clear = Clear the filter
status-active = Active
status-draft = Draft
status-retired = Retired
status-unknown = No status
content-complete = All concepts
content-not-present = Not held
content-example = Examples
content-fragment = Part
content-supplement = Supplement
content-undeclared = Not declared
hierarchy-tree = Tree
hierarchy-no-tree = Hierarchy, no tree
hierarchy-flat = Flat

## Code systems

code-systems-subtitle = Every code system this server offers on FHIR { $version }.
code-systems-system = Code system
code-systems-source = Source
code-systems-versions = Versions
code-systems-content = Content
code-systems-hierarchy = Hierarchy
code-systems-no-match = { $count } code systems are served, and none matches the filter.
code-systems-pages = Code system pages
code-systems-find = Name or canonical
code-systems-new = New code system

## Value sets

value-sets-subtitle = The value sets this server holds for FHIR { $version }.
value-sets-value-set = Value set
value-sets-version = Version
value-sets-status = Status
value-sets-selects = Selects
value-sets-none = No value set is held yet.
value-sets-first = Add the first value set
value-sets-no-match = { $count } value sets are held, and none matches the filter.
value-sets-new = New value set
value-sets-find = Title or canonical
value-sets-pages = Value set pages
selects-codes-of = { $count ->
    [one] 1 code of { $system }
   *[other] { $count } codes of { $system }
}
selects-codes = { $count ->
    [one] 1 code
   *[other] { $count } codes
}
selects-value-sets = { $count ->
    [one] 1 value set
   *[other] { $count } value sets
}
selects-whole-system = the whole code system
selects-nothing-stated = no definition stated
selects-less = , less { $excluded }

## Concept maps

concept-maps-subtitle = The concept maps this server holds for FHIR { $version }.
concept-maps-map = Concept map
concept-maps-mappings = Mappings
concept-maps-mapping-count = { $count ->
    [one] 1 mapping
   *[other] { $count } mappings
}
concept-maps-from-to = { $source } → { $target }
concept-maps-none = No concept map is held yet.
concept-maps-first = Add the first concept map
concept-maps-no-match = { $count } concept maps are held, and none matches the filter.
concept-maps-new = New concept map
concept-maps-pages = Concept map pages

## A resource page

detail-tabs-code-system = Views of this code system
detail-tabs-value-set = Views of this value set
detail-tabs-concept-map = Views of this concept map
detail-overview = Overview
detail-versions = Versions
detail-definition = Definition
detail-expansion = Expansion
detail-mappings = Mappings
detail-history = History
detail-json = JSON
detail-stored = stored on this server
detail-index = offline index
detail-loaded = loaded at start
detail-version-line = Version { $version } · { $source }
detail-more-for = More actions for { $name }
detail-yes = Yes
detail-no = No
detail-json-summary-title = { $path }, summary

## A code system

code-system-no-canonical = The address names no code system
code-system-on-version = Not served on FHIR { $version }
code-system-browse = Browse concepts
code-system-look-up = Look up a code in it
code-system-check = Check a code in it
code-system-expand = Expand as a value set
code-system-edit = Edit
code-system-index-note = Served from an index built offline, so the console cannot edit it and it keeps no history. A new release arrives as a new directory under the index root.
code-system-loaded-note = Read when the server starts, from a resource directory or from what the server carries, so the console cannot edit it and it keeps no history.
code-system-no-resource = The root publishes no CodeSystem resource for this canonical, so there is nothing it says about itself to show.
code-system-computer-name = Computer name
code-system-experimental = Experimental
code-system-publisher = Publisher
code-system-last-changed = Last changed
code-system-description = Description
code-system-content-count = { $content }, { $count } of them
code-system-hierarchy-meaning = Hierarchy meaning
code-system-case-sensitive = Case sensitive
code-system-version-needed = Version needed
code-system-compositional = Compositional grammar
code-system-supplements = Supplements
code-system-property = Property
code-system-property-type = Type
code-system-property-meaning = Meaning
code-system-declared-properties = Declared properties
code-system-details = Details
code-system-structure = Structure
code-system-unnamed-version = no version
code-system-release = Release { $version }
code-system-answers-default = For { $version }, and for requests that name no version.
code-system-answers-only = For { $version } only.
code-system-subsumes = Yes, so Check offers Relation
code-system-no-subsumption = No, so Check offers no Relation
code-system-answers-requests = Answers requests
code-system-subsumption = Subsumption
code-system-languages = Designation languages
code-system-look-up-returns = Properties Look up returns
code-system-read-from = Read from
code-system-filters-of = Filters { $version } accepts
code-system-no-filters = This version declares no filter.
code-system-operators = Operators
code-system-versions-intro = What the server declares it can do with each version.
code-system-json-label = The CodeSystem as JSON
code-system-summary-note = Read with _summary=true: the concepts are left out. Look up reads one.

## A value set

value-set-check = Check a code in it
value-set-expand-all = Expand it in Operations
value-set-selects = What the definition selects
value-set-no-compose = The value set carries no definition, so what it holds is whatever the server built for it.
value-set-loaded-note = Read from a resource directory when the server starts, so the console cannot edit it and it keeps no history.
value-set-open-in-operations = Open in Operations
value-set-no-expansion = The server answered without an expansion.
value-set-concept = Concept
value-set-system = System
value-set-first-page = The first page of the expansion
value-set-shown = { $shown } concepts shown.
value-set-shown-of = Concepts 1 to { $shown } of { $total }.
value-set-expansion-title = The expansion
value-set-json-label = The ValueSet as JSON
clause-no-system = Every system the value sets name
clause-from = From
clause-version = , version { $version }
clause-includes-codes = { $count ->
    [one] Includes 1 code:
   *[other] Includes { $count } codes:
}
clause-excludes-codes = { $count ->
    [one] Excludes 1 code:
   *[other] Excludes { $count } codes:
}
clause-value-sets = { $count ->
    [one] Every code of 1 value set.
   *[other] Every code the { $count } value sets have in common.
}
clause-whole-system = Every code of the code system.
filter-is-a = Includes CONCEPT and everything below it.
filter-descendent-of = Includes everything below CONCEPT.
filter-is-not-a = Includes everything that is not CONCEPT or below it.
filter-child-of = Includes the children of CONCEPT.
filter-generalizes = Includes CONCEPT and everything above it.
filter-equals = Includes CONCEPT.
filter-other = Where { $property } { $op } { $value }.

## A concept map

concept-map-translate = Translate a code through it
concept-map-no-system = an unnamed system
concept-map-to = to
concept-map-every = Every mapping the map writes
concept-map-no-mappings = The map writes no mapping.
concept-map-codes-unmapped = { $unmapped ->
    [one] { $count }, one of them to nothing
   *[other] { $count }, { $unmapped } of them to nothing
}
concept-map-codes-mapped = Codes mapped
concept-map-what-it-maps = What it maps
concept-map-json-label = The ConceptMap as JSON

## The history of a stored resource

history-conflict = Someone changed this resource since you opened it, so the restore was refused.
history-reload = Reload the revisions
history-restored = Revision { $revision } is restored as revision { $written }.
history-created = Created
history-updated = Updated
history-deleted = Deleted
history-revision = Revision
history-changed = Changed
history-interaction = Interaction
history-actions = Actions
history-current = Current
history-revision-n = Revision { $revision }
history-revision-current = Revision { $revision }, current
history-revisions = Revisions
history-intro = The revisions the server holds, read back one at a time.
history-truncated = The latest revisions the server holds. The list stops at 20, so older ones are not shown.
history-restore = Restore
history-restore-n = Restore revision { $revision }
history-restore-question = Restore revision { $revision }?
history-restore-message = The server writes revision { $revision }'s whole resource back as a new revision. Revision { $current } stays in the history. If someone changed the { $kind } since you opened it, the server refuses the restore and nothing changes.
history-earlier = Earlier revision
history-later = Later revision
history-compare = Compare
history-choose-two = Choose two revisions to see what differs.
history-differ = { $count ->
    [0] Revision { $from } and revision { $to } state the same elements.
    [one] 1 element differs between revision { $from } and revision { $to }, leaving out the revision id and time the server sets on every write.
   *[other] { $count } elements differ between revision { $from } and revision { $to }, leaving out the revision id and time the server sets on every write.
}
history-what-changed = What changed
history-compare-title = Compare two revisions

## The operations

ops-title = Operations
ops-subtitle = Run the server's terminology operations; nothing is saved on the server.
ops-tabs = Operations
ops-look-up = Look up
ops-check = Check
ops-expand = Expand
ops-translate = Translate
ops-running = Running
ops-request = The request

## Look up

lookup-system = Code system
lookup-version = Version
lookup-server-version = The server's default
lookup-default-version = { $version } (default)
lookup-code = Code
lookup-code-hint = The system's code
lookup-language = Display language
lookup-server-language = The server's own choice
lookup-before = Type a code and look it up.
lookup-open-in-browser = Open in the concept browser
lookup-raw = The Parameters, as the server sent them
lookup-found = Found
lookup-in = in { $system }
lookup-in-version = in { $system }, version { $version }
lookup-display = Display
lookup-term = Term
lookup-term-language = Language
lookup-use = Use
lookup-designations = Designations
lookup-property = Property
lookup-value = Value
lookup-properties = Properties

## Check

check-what = What to check
check-membership = Membership
check-relation = Relation
check-value-set = Value set
check-its-system = Its code system
check-choose-system = Choose its code system
check-before = Type a code and check it against the value set.
check-needs-system = Choose the code system the code belongs to, then check it.
check-raw = The Parameters, as the server sent them
check-valid = Valid:
check-not-valid = Not valid:
check-is-in = is in { $set }
check-is-not-in = is not in { $set }
check-no-result = The server answered without a result.
check-system-version = { $system }, version { $version }
check-inactive = (inactive)
check-concept = Concept
check-server-says = What the server says
check-code-a = Code A
check-code-b = Code B
check-the-relation = Check the relation
check-relation-before = Type two codes of the code system and check how they relate.
check-no-outcome = The server answered without an outcome.

## Expand

expand-filter = Filter
expand-filter-hint = Text in a designation
expand-page-size = Page size
expand-active-only = Active concepts only
expand-designations = Include designations
expand-before = Choose a value set and expand it.
expand-none = No concept of { $set } matches.
expand-shown-of = Concepts { $first } to { $last } of { $total } in { $set }
expand-shown = Concepts { $first } to { $last } in { $set }
expand-concepts = Concepts
expand-pages = Expansion pages
expand-noun = { $count ->
    [one] concept
   *[other] concepts
}
    .empty = No concepts
expand-applied = Applied parameters

## Translate

translate-map = The map
translate-any-map = Any map the server holds
translate-target = Target code system
translate-any-target = Any
translate-before = Type a code and translate it.
translate-the-maps = the server's maps
translate-translated = Translated
translate-through = through { $map }:
translate-matches = { $count ->
    [0] no match
    [one] 1 match
   *[other] { $count } matches
}
translate-target-column = Target
translate-relationship = Relationship
translate-matches-title = Matches
translate-comment = Comment
translate-no-target = No target

## The concept browser

browse-title = Concept browser
browse-subtitle = Search a code system, read a concept and walk its hierarchy.
browse-hierarchy = Hierarchy
browse-concept = The concept
browse-search = Search
browse-choose-system = Choose a code system
browse-choose-system-tree = Choose a code system to walk its hierarchy.
browse-choose-concept = Choose a concept to read it.
browse-choose-concept-tree = Choose a concept to walk the hierarchy around it.
browse-choose-system-first = Choose a code system first.
browse-open = Open
browse-search-hint = Search the designations
browse-count = { $count ->
    [one] 1 concept
   *[other] { $count } concepts
}
browse-matches = Matching concepts
browse-all-properties = All properties
browse-parent = Parent
browse-children = Children
browse-read-from = · read from version { $version }

## Find

find-subtitle = What you typed, read by its shape, and the screens that take it.
find-read-as = reads as { $subject } from its shape alone. No code system, value set or concept map on FHIR { $version } has it as a name or a canonical.
find-nothing = Nothing on this root matches { $typed }.
find-unfinished = The server did not answer every read ({ $reads }), so the matches may be short.
find-look-up = Look up { $code }
find-look-up-detail = Opens Look up with the code filled in. Name the system it belongs to.
find-check = Check { $code }
find-check-detail = Opens Check with the code filled in, ready for a value set.
find-translate = Translate { $code }
find-translate-detail = Opens Translate with the code filled in. Name the system it belongs to.
find-search = Search the designations for { $code }

## The editors

editor-new = Not saved yet.
editor-version = Version { $version }.
editor-opened = Version { $version }, opened { $since }.
editor-unchanged = No changes yet.
editor-changed = Changes not saved yet.
editor-refused-line = Not saved. Read what the server said above.
editor-expired-line = Not saved: sign in again first.
editor-dock = Save
editor-discard = Discard
editor-sign-in-again = Sign in again
editor-expired = Your session ended, so the save was not sent. Sign in again, then open the editor and make the change again.
editor-conflict = Someone changed this resource since you opened it, so the save was refused.
editor-reload = Reload the current version
editor-saved-title = Saved
editor-saved = Saved. The server now holds version { $version }.
editor-edit = Edit
editor-lifecycle-active = Active
editor-lifecycle-experimental = Experimental
editor-lifecycle-deprecated = Deprecated
editor-lifecycle-retired = Retired
editor-forbidden-code-system = Edit a code system
editor-code-system-needs = Give the code system a canonical, a status and a content mode to save it.
editor-create-code-system = Create this code system
editor-save-code-system = Save this code system
editor-new-code-system = New code system
editor-metadata = Metadata
editor-canonical = Canonical URL
editor-title = Title
editor-name = Name
editor-business-version = Business version
editor-status = Publication status
editor-case-sensitive = Codes are case sensitive
editor-property-code = Property code
editor-property-type = Type
editor-remove-property = Remove the property { $code }
editor-property-kept = The editor writes this property itself, from each concept's lifecycle.
editor-declared-properties = Declared properties
editor-declare-property = Declare a property
editor-no-properties = No property declared yet.
editor-concept-code = Code
editor-concept-display = Display
editor-concept-parent = Parent
editor-concept-lifecycle = Lifecycle
editor-remove-concept = Remove this new concept
editor-top-level = top level
editor-display-of = Display of { $code }
editor-lifecycle-of = Lifecycle of { $code }
editor-concepts = Concepts
editor-add-concept = Add a concept
editor-retired-not-removed = A concept is retired, never removed, so a published code keeps meaning what it meant.
editor-open-code-system = Open the code system
editor-forbidden-value-set = Compose a value set
editor-value-set-needs = Give the value set a canonical, a status and at least one complete include to save it.
editor-create-value-set = Create this value set
editor-save-value-set = Save this value set
editor-new-value-set = New value set
editor-inactive-codes = Inactive codes
editor-inactive-server = Leave it to the server
editor-inactive-include = Include them
editor-inactive-exclude = Leave them out
editor-published-value-sets = Published value sets
editor-no-system = No code system
editor-filter-property = Property
editor-filter-operator = Operator
editor-remove-filter = Remove this filter
editor-remove-code = Remove this code
editor-value-set-canonical = Value set canonical
editor-remove-value-set = Remove this value set
editor-filters = Filters
editor-add-filter = Add filter
editor-codes = Codes
editor-add-code = Add code
editor-remove-clause = Remove this clause
editor-includes-excludes = Includes and excludes
editor-includes-excludes-intro = Includes add codes; excludes take them back out.
editor-add-include = Add an include
editor-add-exclude = Add an exclude
editor-add-value-set = Add a published value set
editor-run-preview = Run the preview
editor-preview-before = Run the preview to see what this selects.
editor-preview = The preview
editor-open-value-set = Open the value set
editor-forbidden-concept-map = Edit a concept map
editor-create-concept-map = Create this concept map
editor-save-concept-map = Save this concept map
editor-new-concept-map = New concept map
editor-no-system-yet = no system yet
editor-group-title = Group { $place }: { $source } to { $target }
editor-remove-group = Remove this group
editor-source-system = Source system
editor-target-system = Target system
editor-codes-count = { $count ->
    [one] 1 code
   *[other] { $count } codes
}
editor-add-map-code = Add a code
editor-add-group = Add a group
editor-group-intro = A group maps codes of one system to codes of another, and carries at least one code.
editor-source-code = Source code
editor-no-map = Maps to nothing
editor-target-code = Target code
editor-no-target = No target
editor-choose-relationship = Choose a relationship
editor-optional = Optional
editor-remove-target = Remove this target
editor-add-target = Add a target
editor-remove-map-code = Remove this code
editor-open-concept-map = Open the concept map
editor-back-to-resource = Back to the resource
editor-unmanaged = The server read this resource from its resource directory when it started, so it keeps no version the console could update. Change the file it was read from instead.

## The console's own form routes

post-no-preference = The post names no preference.
post-one-preference = A post keeps one preference.
post-not-offered = The preference or its value is not one the console offers.
post-sidebar = The navigation is either the rail or full.
post-version = The FHIR version is not one this server serves.
server-form-too-large = The form is larger than the console takes.

## Synchronisation

shell-synchronisation = Synchronisation
sync-subtitle = The runs the synchronisation service made, newest first, read from its admin listener.
sync-runs = Runs
sync-column-run = Run
sync-column-trigger = Trigger
sync-column-outcome = Outcome
sync-column-did = What it did
sync-column-findings = Findings
sync-trigger-schedule = Schedule
sync-trigger-manual = Run now
sync-trigger-run-once = Run once
sync-trigger-activate = Activate
sync-outcome-ok = Succeeded
sync-outcome-failed = Failed
sync-outcome-other = Ended another way
sync-did-nothing = Nothing new
sync-did = { $taken ->
    [one] 1 release taken
   *[other] { $taken } releases taken
}, { $activated } activated
sync-findings-count = { $count ->
    [0] No findings
    [one] 1 finding
   *[other] { $count } findings
}
sync-no-runs = The service has recorded no run yet.
sync-run-now = Run now
sync-run-now-question = Run the synchronisation now?
sync-run-now-message = The service reads the feed, takes what you subscribed to and do not hold, builds it, and asks the server to reload. A run that fails leaves the served set as it was. This page answers once the run has finished, which can take as long as a build.
sync-more = More synchronisation actions
sync-activate = Activate what is staged
sync-activate-question = Activate what is staged?
sync-activate-message = The service puts what it holds staged in front of the server and asks the server to reload.
sync-ran-ok = The run { $id } finished and succeeded.
sync-ran-failed = The run { $id } finished and failed. Open it to read why.
sync-open-run = Open the run
sync-run-title = Run of { $when }
sync-took = Took { $duration }.
sync-duration-minutes = { $minutes } min { $seconds } s
sync-duration-seconds = { $seconds } s
sync-tabs = Views of this run
sync-tab-summary = Summary
sync-tab-findings = Findings
sync-download = Download the record
sync-what = What the run did
sync-fact-trigger = Trigger
sync-fact-started = Started
sync-fact-ended = Ended
sync-fact-sources = Sources
sync-fact-taken = Taken
sync-fact-left = Left behind
sync-fact-activation = Activation
sync-fact-revalidation = Revalidation
sync-fact-errors = What went wrong
sync-source-line = { $source }: the feed offered { $seen } entries
sync-source-line-api = { $source }: the feed offered { $seen } entries, and its FHIR API listed { $listed } resources
sync-taken-entry = { $title } { $version }
sync-taken-failed = { $title } { $version }: { $error }
sync-nothing = Nothing
sync-left-line = { $skipped } entries for another reason, { $unsubscribed } outside your subscription
sync-activation-line = { $activated } activated, { $staged } still staged, { $reloads } reloads asked of the server
sync-activation-failed = { $activated } activated, then it failed: { $error }
sync-revalidation-line = { $resources } locally written resources read, { $checked } codes checked: { $statement }
sync-revalidation-skipped = Not run: { $reason }
sync-effect = Effect on your content
sync-effect-intro = What the release did to the codes your own resources name.
sync-column-resource = Resource
sync-column-code = Code
sync-column-change = What changed
sync-column-release = Release
sync-kind-inactive = The release made this code inactive
sync-kind-absent = The release no longer carries this code
sync-kind-outside-value-set = The code no longer falls inside the value set it was included through
sync-release-none = No release
sync-no-findings = The run found nothing that changed your own content.
sync-not-found = The synchronisation service holds no record of the run { $id }.

## Reload and delete

server-more = More server actions
server-reload = Reload the served set
server-reload-question = Reload the served set?
server-reload-message = The server opens every package and resource folder again and swaps the whole set only when every one opens. A request in flight finishes on the old set. The resources stored on this server stay as they are.
server-reloaded = The server reloaded its served set and answered { $status }.
server-reload-refused = The server answered the reload { $status }, so the served set stays as it was: { $body }
value-set-delete = Delete this value set
value-set-delete-question = Delete { $name }?
value-set-delete-message = The server removes the value set, and a request for its canonical then finds nothing. Its revisions stay in the server's history, so it can be restored.
value-set-delete-confirm = Delete the value set
value-sets-deleted = The value set { $id } was deleted. Its revisions stay in the server's history.
config-admin = Server admin listener
config-admin-line = The terminology server's admin listener, which Reload the served set on Server posts to.
