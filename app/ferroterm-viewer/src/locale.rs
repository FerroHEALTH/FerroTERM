// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The locale the console renders in, and the catalog of its own words.
//!
//! The console's words are messages of the Fluent catalogs under the crate's
//! `locales/` directory, one directory per language
//! (`locales/en/ferroterm-viewer.ftl`), embedded at build time by
//! `fluent_templates::static_loader!`, the mechanism `ferrohealth_ui`
//! catalogs its own words with. English is the one catalog that ships and the
//! one every lookup falls back to; a language joins as one catalog file and
//! no code change.
//!
//! The server chooses the locale once per request: the best language of the
//! request's `Accept-Language` the catalog holds, else the configured
//! `[server] locale`. [`crate::app::App`] provides it to both halves through
//! `ferrohealth_ui::i18n::provide_locale`, so the console's words and the
//! components' words follow the same choice, and the document writes it as
//! `<html lang>` (WCAG 2.2 SC 3.1.1). No specification governs the catalog
//! layout or the choice: our own design.

use ferrohealth_ui::i18n::{Arg, Locale, Message, locale};
use fluent_templates::Loader;

fluent_templates::static_loader! {
    static CATALOG = {
        locales: "./locales",
        fallback_language: "en",
        // NOTE: Fluent's guide, "Bidirectionality": the isolation marks around a
        // placeable are for mixed-direction text; the English catalog draws none.
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

/// The console's catalogs, every language it ships.
#[must_use]
pub(crate) fn catalog() -> &'static (dyn Loader + Send + Sync) {
    &*CATALOG
}

/// Declares the console's messages: each variant names one message of the
/// catalog by its id.
macro_rules! messages {
    ($($(#[$meta:meta])+ $name:ident = $id:literal,)+) => {
        /// A string the console draws, by its message in the catalog.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[cfg_attr(
            target_arch = "wasm32",
            expect(dead_code, reason = "a message the server half alone draws is never named in the browser")
        )]
        pub(crate) enum Msg {
            $($(#[$meta])+ $name,)+
        }

        impl Msg {
            /// Every message, in declaration order.
            #[cfg(test)]
            pub(crate) const ALL: &[Self] = &[$(Self::$name,)+];

            /// The message's id in the catalog.
            #[must_use]
            pub(crate) const fn id(self) -> &'static str {
                match self {
                    $(Self::$name => $id,)+
                }
            }
        }
    };
}

messages! {
    /// The document's description of the console.
    #[cfg_attr(
        target_arch = "wasm32",
        expect(dead_code, reason = "the server half alone writes the document")
    )]
    Description = "description",
    /// The title of the screen an address that names no page reaches.
    NotFoundTitle = "not-found-title",
    /// That screen's heading.
    NotFoundHeading = "not-found-heading",
    /// The sentence of that screen.
    NotFoundBody = "not-found-body",
    /// That screen's way back.
    NotFoundBack = "not-found-back",
    /// `May edit ConceptMap`
    MayEditConceptmap = "may-edit-conceptmap",
    /// `May edit ValueSet`
    MayEditValueset = "may-edit-valueset",
    /// `May edit CodeSystem`
    MayEditCodesystem = "may-edit-codesystem",
    /// `May edit ValueSet and ConceptMap`
    MayEditValuesetConceptmap = "may-edit-valueset-conceptmap",
    /// `May edit CodeSystem and ConceptMap`
    MayEditCodesystemConceptmap = "may-edit-codesystem-conceptmap",
    /// `May edit CodeSystem and ValueSet`
    MayEditCodesystemValueset = "may-edit-codesystem-valueset",
    /// `May edit CodeSystem, ValueSet and ConceptMap`
    MayEditCodesystemValuesetConceptmap = "may-edit-codesystem-valueset-conceptmap",
    /// `Read only: this account has no editing permission`
    ReadOnlyAccountHasNoEditing = "read-only-account-has-no-editing",
    /// `Every group carries at least one code, which is what a group is for.`
    EveryGroupCarriesLeastOneCode = "every-group-carries-least-one-code",
    /// `Every group names the system it maps from and the system it maps to.`
    EveryGroupNamesSystemMapsFrom = "every-group-names-system-maps-from",
    /// `A concept map needs a publication status.`
    ConceptMapNeedsPublicationStatus = "concept-map-needs-publication-status",
    /// `A concept map needs a canonical before it can be saved.`
    ConceptMapNeedsCanonicalBeforeCan = "concept-map-needs-canonical-before-can",
    /// `1 element differs.`
    N1ElementDiffers = "n-1-element-differs",
    /// `These two versions state the same elements.`
    TheseTwoVersionsStateSameElements = "these-two-versions-state-same-elements",
    /// `One of these two versions carries no resource, so there is nothing to compare.`
    OneTheseTwoVersionsCarriesNo = "one-these-two-versions-carries-no",
    /// `Choose two versions above to compare them.`
    ChooseTwoVersionsAboveCompareThem = "choose-two-versions-above-compare-them",
    /// `This server's capability statement declares neither the history interaction nor the...`
    ServerSCapabilityStatementDeclaresNeither = "server-s-capability-statement-declares-neither",
    /// `This server holds no earlier version of this resource.`
    ServerHoldsNoEarlierVersionResource = "server-holds-no-earlier-version-resource",
    /// `What the root says it serves`
    WhatRootSaysServes = "what-root-says-serves",
    /// `Software`
    Software = "software",
    /// `not read`
    NotRead = "not-read",
    /// `not served`
    NotServed = "not-served",
    /// `This server did not expand the value set this control's codes come from, so it offers...`
    ServerDidNotExpandValueSet = "server-did-not-expand-value-set",
    /// `Choose one`
    ChooseOne = "choose-one",
    /// `The answer did not read`
    AnswerDidNotRead = "answer-did-not-read",
    /// `The request got no answer`
    RequestGotNoAnswer = "request-got-no-answer",
    /// `The server refused the request`
    ServerRefusedRequest = "server-refused-request",
    /// `Another version`
    AnotherVersion = "another-version",
    /// `The one this server resolves to`
    OneServerResolves = "one-server-resolves",
    /// `Another canonical`
    AnotherCanonical = "another-canonical",
    /// `Choose one this server holds`
    ChooseOneServerHolds = "choose-one-server-holds",
    /// `for example en or nl-NL`
    ExampleEnNlNl = "example-en-nl-nl",
    /// `Unreachable`
    Unreachable = "unreachable",
    /// `Serving`
    Serving = "serving",
    /// `Checking the server`
    CheckingTheServer = "checking-the-server",
    /// `History of this resource`
    HistoryOfThisResource = "history-of-this-resource",
    /// `not declared`
    NotDeclared = "not-declared",
    /// `Reloaded. The server stated no version for it.`
    ReloadedServerStatedNoVersion = "reloaded-server-stated-no-version",
    /// `Reading the current version from the server.`
    ReadingCurrentVersionFromServer = "reading-current-version-from-server",
    /// `Reload the current version`
    ReloadTheCurrentVersion = "reload-the-current-version",
    /// `Each one is the address that made it. They live in this page and are never sent anywhere.`
    EachOneAddressMadeTheyLive = "each-one-address-made-they-live",
    /// `The runs made on this page`
    RunsMadePage = "runs-made-page",
    /// `Screens`
    Screens = "screens",
    /// `Console`
    Console = "console",
    /// `Signed in`
    SignedIn = "signed-in",
    /// `Sign in`
    SignIn = "sign-in",
    /// `FHIR`
    Fhir = "fhir",
    /// `FHIR version`
    FhirVersion = "fhir-version",
    /// `A code, a canonical or a phrase`
    CodeCanonicalPhrase = "code-canonical-phrase",
    /// `Find a code, a canonical or a phrase`
    FindCodeCanonicalPhrase = "find-code-canonical-phrase",
    /// `Server`
    Server = "server",
    /// `Operations`
    Operations = "operations",
    /// `Browse`
    Browse = "browse",
    /// `Authoring`
    Authoring = "authoring",
    /// `About this server`
    AboutThisServer = "about-this-server",
    /// `Concept map editor`
    ConceptMapEditor = "concept-map-editor",
    /// `Value set composer`
    ValueSetComposer = "value-set-composer",
    /// `Code system editor`
    CodeSystemEditor = "code-system-editor",
    /// `Translate a code`
    TranslateACode = "translate-a-code",
    /// `Expand a value set`
    ExpandAValueSet = "expand-a-value-set",
    /// `Check a code`
    CheckACode = "check-a-code",
    /// `Look up a code`
    LookUpACode = "look-up-a-code",
    /// `Concept maps`
    ConceptMaps = "concept-maps",
    /// `Value sets`
    ValueSets = "value-sets",
    /// `Code systems`
    CodeSystems = "code-systems",
    /// `Dashboard`
    Dashboard = "dashboard",
    /// `Nothing here`
    NothingHere = "nothing-here",
    /// `the server named no artifact for this version`
    ServerNamedNoArtifactVersion = "server-named-no-artifact-version",
    /// `this version was not read from a built artifact`
    VersionWasNotReadFromBuilt = "version-was-not-read-from-built",
    /// `the server declared neither yes nor no`
    ServerDeclaredNeitherYesNorNo = "server-declared-neither-yes-nor-no",
    /// `no`
    No = "no",
    /// `yes`
    Yes = "yes",
    /// `this FHIR version has no such element, or the server declared none`
    FhirVersionHasNoSuchElement = "fhir-version-has-no-such-element",
    /// `this server declared no version for the code system`
    ServerDeclaredNoVersionCodeSystem = "server-declared-no-version-code-system",
    /// `Declared without a canonical URI`
    DeclaredWithoutCanonicalUri = "declared-without-canonical-uri",
    /// `Validate`
    Validate = "validate",
    /// `a screen needs the canonical this system was declared without`
    ScreenNeedsCanonicalSystemWasDeclared = "screen-needs-canonical-system-was-declared",
    /// `Actions`
    Actions = "actions",
    /// `Read from`
    ReadFrom = "read-from",
    /// `Subsumes`
    Subsumes = "subsumes",
    /// `Content`
    Content = "content",
    /// `Versions`
    Versions = "versions",
    /// `Pages of code systems`
    PagesOfCodeSystems = "pages-of-code-systems",
    /// `The code systems this root serves`
    CodeSystemsRootServes = "code-systems-root-serves",
    /// `No code system this root serves matches the filter.`
    NoCodeSystemRootServesMatches = "no-code-system-root-serves-matches",
    /// `No match`
    NoMatch = "no-match",
    /// `Code system`
    CodeSystem2 = "code-system-2",
    /// `Concepts defined`
    ConceptsDefined = "concepts-defined",
    /// `Supplements`
    Supplements = "supplements",
    /// `Compositional grammar`
    CompositionalGrammar = "compositional-grammar",
    /// `Hierarchy meaning`
    HierarchyMeaning = "hierarchy-meaning",
    /// `Case sensitive`
    CaseSensitive = "case-sensitive",
    /// `A code needs its version`
    CodeNeedsVersion = "code-needs-version",
    /// `Description`
    Description2 = "description-2",
    /// `Last changed`
    LastChanged = "last-changed",
    /// `Publisher`
    Publisher = "publisher",
    /// `Experimental`
    Experimental = "experimental",
    /// `Status`
    Status = "status",
    /// `Version`
    Version = "version",
    /// `Title`
    Title = "title",
    /// `Name`
    Name = "name",
    /// `, and`
    And = "and",
    /// `This clause takes back out every code that`
    ClauseTakesBackOutEveryCode = "clause-takes-back-out-every-code",
    /// `This clause draws in every code that`
    ClauseDrawsEveryCode = "clause-draws-every-code",
    /// `comes from the code system named here`
    ComesFromCodeSystemNamedHere = "comes-from-code-system-named-here",
    /// `is in the value set named here`
    ValueSetNamedHere = "value-set-named-here",
    /// `Parameters`
    Parameters = "parameters",
    /// `This clause lists codes and filters at once. Split it into two clauses: they union.`
    ClauseListsCodesFiltersOnceSplit = "clause-lists-codes-filters-once-split",
    /// `This clause lists codes or filters, which name a code system to be read from.`
    ClauseListsCodesFiltersWhichName = "clause-lists-codes-filters-which-name",
    /// `This clause names neither a value set nor a code system, so it selects nothing.`
    ClauseNamesNeitherValueSetNor = "clause-names-neither-value-set-nor",
    /// `Groups`
    Groups = "groups",
    /// `Translates to`
    TranslatesTo = "translates-to",
    /// `Translates from`
    TranslatesFrom = "translates-from",
    /// `Purpose`
    Purpose = "purpose",
    /// `the address is not below the server's root`
    AddressNotBelowServerSRoot = "address-not-below-server-s-root",
    /// `the server sent no diagnostic`
    ServerSentNoDiagnostic = "server-sent-no-diagnostic",
    /// `the run recorded no kind for this finding`
    RunRecordedNoKindFinding = "run-recorded-no-kind-finding",
    /// `the code no longer falls inside the value set it was included through`
    CodeNoLongerFallsInsideValue = "code-no-longer-falls-inside-value",
    /// `the release no longer carries this code`
    ReleaseNoLongerCarriesCode = "release-no-longer-carries-code",
    /// `the release made this code inactive`
    ReleaseMadeCodeInactive = "release-made-code-inactive",
    /// `Relationship`
    Relationship = "relationship",
    /// `Equivalence`
    Equivalence = "equivalence",
    ///  The concept is inactive in its code system.
    ConceptInactiveCodeSystem = "concept-inactive-code-system",
    /// `The server answered no result.`
    ServerAnsweredNoResult = "server-answered-no-result",
    /// `The server answered result false: the code is not valid.`
    ServerAnsweredResultFalseCodeNot = "server-answered-result-false-code-not",
    /// `The server answered result true: the code is valid.`
    ServerAnsweredResultTrueCodeValid = "server-answered-result-true-code-valid",
    /// `Locked to date`
    LockedToDate = "locked-to-date",
    /// `Inactive codes selected`
    InactiveCodesSelected = "inactive-codes-selected",
    /// `Immutable`
    Immutable = "immutable",
    /// `The server did not answer. The request is below, to retry by hand.`
    ServerDidNotAnswerRequestBelow = "server-did-not-answer-request-below",
    /// `The server refused the change, in its own words below.`
    ServerRefusedChangeOwnWordsBelow = "server-refused-change-own-words-below",
    /// `Someone else changed this resource since it was opened. Reload it and apply the change...`
    SomeoneElseChangedResourceSinceWas = "someone-else-changed-resource-since-was",
    /// `This account does not carry the permission for this change.`
    AccountDoesNotCarryPermissionChange = "account-does-not-carry-permission-change",
    /// `Sign in again to make this change.`
    SignAgainMakeChange = "sign-again-make-change",
    /// `Sends the phrase as the filter parameter, in the code system you pick there.`
    SendsPhraseFilterParameterCodeSystem = "sends-phrase-filter-parameter-code-system",
    /// `Search the designations`
    SearchTheDesignations = "search-the-designations",
    /// `Searches the concept browser for it, in the code system you pick there.`
    SearchesConceptBrowserCodeSystemYou = "searches-concept-browser-code-system-you",
    /// `Find it in a hierarchy`
    FindHierarchy = "find-hierarchy",
    /// `Opens the runner with the code filled in. Name the system it belongs to.`
    OpensRunnerCodeFilledNameSystem = "opens-runner-code-filled-name-system",
    /// `Translate this code`
    TranslateThisCode = "translate-this-code",
    /// `Validate this code`
    ValidateThisCode = "validate-this-code",
    /// `Opens the runner with the canonical filled in, ready for a code.`
    OpensRunnerCanonicalFilledReadyCode = "opens-runner-canonical-filled-ready-code",
    /// `Validate a code in it`
    ValidateCode = "validate-code",
    /// `Sends the canonical as the url parameter of ValueSet/$expand.`
    SendsCanonicalUrlParameterValuesetExpand = "sends-canonical-url-parameter-valueset-expand",
    /// `Expand it as a value set`
    ExpandValueSet = "expand-value-set",
    /// `Walks the hierarchy, where the served version declares an operator to walk it by.`
    WalksHierarchyWhereServedVersionDeclares = "walks-hierarchy-where-served-version-declares",
    /// `Browse its concepts`
    BrowseItsConcepts = "browse-its-concepts",
    /// `Reads what this root declares about it, and the CodeSystem it publishes for it.`
    ReadsWhatRootDeclaresAboutCodesystem = "reads-what-root-declares-about-codesystem",
    /// `Open the code system`
    OpenTheCodeSystem = "open-the-code-system",
    /// `a phrase`
    APhrase = "a-phrase",
    /// `a code`
    ACode = "a-code",
    /// `a canonical`
    ACanonical = "a-canonical",
    /// `, which carries no id to read it by`
    WhichCarriesNoIdRead = "which-carries-no-id-read",
    /// `This root holds no resource matching the search.`
    RootHoldsNoResourceMatchingSearch = "root-holds-no-resource-matching-search",
    /// `Search`
    Search = "search",
    /// `Search this root`
    SearchThisRoot = "search-this-root",
    /// `The version search parameter. Left empty, every version this root holds matches.`
    VersionSearchParameterLeftEmptyEvery = "version-search-parameter-left-empty-every",
    /// `Canonical`
    Canonical = "canonical",
    /// `What each FHIR root declares, the evidence this build ships, and this console's settings.`
    WhatEachFhirRootDeclaresEvidence = "what-each-fhir-root-declares-evidence",
    /// `The four FHIR versions`
    TheFourFhirVersions = "the-four-fhir-versions",
    /// `Settings`
    Settings = "settings",
    /// `The evidence this build ships`
    EvidenceBuildShips = "evidence-build-ships",
    /// `Choose a code system`
    ChooseACodeSystem = "choose-a-code-system",
    /// `This server answers no child for this concept.`
    ServerAnswersNoChildConcept = "server-answers-no-child-concept",
    /// `Nothing below`
    NothingBelow = "nothing-below",
    /// `Reading the children`
    ReadingTheChildren = "reading-the-children",
    /// `Arrow keys walk it. Enter reads the concept.`
    ArrowKeysWalkEnterReadsConcept = "arrow-keys-walk-enter-reads-concept",
    /// `Below this concept`
    BelowThisConcept = "below-this-concept",
    /// `The concepts below this one`
    ConceptsBelowOne = "concepts-below-one",
    /// `Nothing declared`
    NothingDeclared = "nothing-declared",
    /// `This server answers no property for this concept.`
    ServerAnswersNoPropertyConcept = "server-answers-no-property-concept",
    /// `Property`
    Property = "property",
    /// `Value`
    Value = "value",
    /// `Meaning`
    Meaning = "meaning",
    /// `Properties`
    Properties = "properties",
    /// `This server holds no designation for this concept.`
    ServerHoldsNoDesignationConcept = "server-holds-no-designation-concept",
    /// `Term`
    Term = "term",
    /// `Language`
    Language = "language",
    /// `Use`
    Use = "use",
    /// `Designations`
    Designations = "designations",
    /// `This server declares no child for this concept.`
    ServerDeclaresNoChildConcept = "server-declares-no-child-concept",
    /// `Children`
    Children = "children",
    /// `This server declares no parent for this concept.`
    ServerDeclaresNoParentConcept = "server-declares-no-parent-concept",
    /// `Parents`
    Parents = "parents",
    /// `Choose a concept from the search results to read its designations and properties.`
    ChooseConceptFromSearchResultsRead = "choose-concept-from-search-results-read",
    /// `The concept`
    TheConcept = "the-concept",
    /// `Reading the concept`
    ReadingTheConcept = "reading-the-concept",
    /// `Concepts the search matched`
    ConceptsTheSearchMatched = "concepts-the-search-matched",
    /// `No concept matched this filter in this code system.`
    NoConceptMatchedFilterCodeSystem = "no-concept-matched-filter-code-system",
    /// `1 concept matched.`
    N1ConceptMatched = "n-1-concept-matched",
    /// `No concept matched.`
    NoConceptMatched = "no-concept-matched",
    /// `Searching the code system`
    SearchingTheCodeSystem = "searching-the-code-system",
    /// `Sent as the filter parameter of $expand over a value set that selects this code...`
    SentFilterParameterExpandOverValue = "sent-filter-parameter-expand-over-value",
    /// `Filter`
    Filter = "filter",
    /// `This version declares no designation language, so the server chooses the display and...`
    VersionDeclaresNoDesignationLanguageSo = "version-declares-no-designation-language-so",
    /// `The languages this version declares. The choice is sent as displayLanguage on every...`
    LanguagesVersionDeclaresChoiceSentDisplaylanguage = "languages-version-declares-choice-sent-displaylanguage",
    /// `Display language`
    DisplayLanguage = "display-language",
    /// `The server's own choice`
    ServerSOwnChoice = "server-s-own-choice",
    /// `Hierarchy`
    Hierarchy = "hierarchy",
    /// `This version declares no hierarchy filter operator, so its concepts are a flat list...`
    VersionDeclaresNoHierarchyFilterOperator = "version-declares-no-hierarchy-filter-operator",
    /// `This root's terminology capabilities declare no such code system, or no such version...`
    RootSTerminologyCapabilitiesDeclareNo = "root-s-terminology-capabilities-declare-no",
    /// `Reading the terminology capabilities`
    ReadingTheTerminologyCapabilities = "reading-the-terminology-capabilities",
    /// `The code system being browsed`
    CodeSystemBeingBrowsed = "code-system-being-browsed",
    /// `What is browsed`
    WhatIsBrowsed = "what-is-browsed",
    /// `Search a code system, read a concept, walk its hierarchy.`
    SearchCodeSystemReadConceptWalk = "search-code-system-read-concept-walk",
    /// `Browse concepts`
    BrowseConcepts = "browse-concepts",
    /// `This root publishes no CodeSystem resource for this canonical. A system served from a...`
    RootPublishesNoCodesystemResourceCanonical = "root-publishes-no-codesystem-resource-canonical",
    /// `Not published as a resource`
    NotPublishedResource = "not-published-resource",
    /// `Filters $expand accepts`
    FiltersExpandAccepts = "filters-expand-accepts",
    /// `Operators`
    Operators = "operators",
    /// `no operator declared`
    NoOperatorDeclared = "no-operator-declared",
    /// `This version declares no $expand filter.`
    VersionDeclaresNoExpandFilter = "version-declares-no-expand-filter",
    /// `No filter`
    NoFilter = "no-filter",
    /// `none declared`
    NoneDeclared = "none-declared",
    /// `$lookup properties`
    LookupProperties = "lookup-properties",
    /// `Designation languages`
    DesignationLanguages = "designation-languages",
    /// `Answers`
    Answers = "answers",
    /// `a request naming this version`
    RequestNamingVersion = "request-naming-version",
    /// `an unversioned request`
    AnUnversionedRequest = "an-unversioned-request",
    /// `This server declares no version for this code system.`
    ServerDeclaresNoVersionCodeSystem = "server-declares-no-version-code-system",
    /// `No version declared`
    NoVersionDeclared = "no-version-declared",
    /// `This root's terminology capabilities do not name this code system. Another FHIR...`
    RootSTerminologyCapabilitiesDoNot = "root-s-terminology-capabilities-do-not",
    /// `The CodeSystem, as the server sent it`
    CodesystemServerSent = "codesystem-server-sent",
    /// `Reading the published CodeSystem`
    ReadingThePublishedCodesystem = "reading-the-published-codesystem",
    /// `What this server can do with it`
    WhatServerCanDo = "what-server-can-do",
    /// `What the code system says it is`
    WhatCodeSystemSays = "what-code-system-says",
    /// `Expand`
    Expand = "expand",
    /// `Screens for this code system`
    ScreensCodeSystem = "screens-code-system",
    /// `Edit`
    Edit = "edit",
    /// `Subsumption`
    Subsumption = "subsumption",
    /// `Default version`
    DefaultVersion = "default-version",
    /// `This address names no code system`
    AddressNamesNoCodeSystem = "address-names-no-code-system",
    /// `Views of this code system`
    ViewsCodeSystem = "views-code-system",
    /// `JSON`
    Json = "json",
    /// `Server support`
    ServerSupport = "server-support",
    /// `Definition`
    Definition = "definition",
    /// `The ConceptMap, as the server sent it`
    ConceptmapServerSent = "conceptmap-server-sent",
    /// `Reading the concept map`
    ReadingTheConceptMap = "reading-the-concept-map",
    /// `Target code`
    TargetCode = "target-code",
    /// `no target`
    NoTarget = "no-target",
    /// `Filter by code or display`
    FilterCodeDisplay = "filter-code-display",
    /// `Every mapping the map writes`
    EveryMappingMapWrites = "every-mapping-map-writes",
    /// `No mapping`
    NoMapping = "no-mapping",
    /// `No mapping matches.`
    NoMappingMatches = "no-mapping-matches",
    /// `What the map maps`
    WhatTheMapMaps = "what-the-map-maps",
    /// `Codes mapped`
    CodesMapped = "codes-mapped",
    /// `To`
    To = "to",
    /// `What it maps is whatever the server holds for it, not a table written here.`
    WhatMapsWhateverServerHoldsNot = "what-maps-whatever-server-holds-not",
    /// `No group`
    NoGroup = "no-group",
    /// `What it declares`
    WhatItDeclares = "what-it-declares",
    /// `Mappings`
    Mappings = "mappings",
    /// `Views of this concept map`
    ViewsConceptMap = "views-concept-map",
    /// `Source code`
    SourceCode = "source-code",
    /// `From`
    From = "from",
    /// `Saved. The server stated no version for it.`
    SavedServerStatedNoVersion = "saved-server-stated-no-version",
    /// `Save this concept map`
    SaveThisConceptMap = "save-this-concept-map",
    /// `Create this concept map`
    CreateThisConceptMap = "create-this-concept-map",
    /// `An update states the version it replaces, so a change made elsewhere since this form...`
    UpdateStatesVersionReplacesSoChange = "update-states-version-replaces-so-change",
    /// `Save`
    Save = "save",
    /// `Saving`
    Saving = "saving",
    /// `the saved map, because this server did not take the map inline`
    SavedMapBecauseServerDidNot = "saved-map-because-server-did-not",
    /// `the map on this screen`
    MapScreen = "map-screen",
    /// `Translating`
    Translating = "translating",
    /// `This code has no system to translate from yet.`
    CodeHasNoSystemTranslateFrom = "code-has-no-system-translate-from",
    /// `System`
    System = "system",
    /// `Display`
    Display = "display",
    /// `Code`
    Code = "code",
    /// `The server stated no result.`
    ServerStatedNoResult = "server-stated-no-result",
    /// `The server translated nothing for this code.`
    ServerTranslatedNothingCode = "server-translated-nothing-code",
    /// `The server translated the code.`
    ServerTranslatedCode = "server-translated-code",
    /// `This server did not take the map inline, so the preview ran against the saved map.`
    ServerDidNotTakeMapInline = "server-did-not-take-map-inline",
    /// `Translated through the map on this screen, sent with the request.`
    TranslatedThroughMapScreenSentRequest = "translated-through-map-screen-sent-request",
    /// `Searching`
    Searching = "searching",
    /// `Comment`
    Comment = "comment",
    /// `Target display`
    TargetDisplay = "target-display",
    /// `Remove this target`
    RemoveThisTarget = "remove-this-target",
    /// `target code`
    TargetCode2 = "target-code-2",
    /// `Add a target`
    AddATarget = "add-a-target",
    /// `Maps to`
    MapsTo = "maps-to",
    /// `This code maps to nothing (noMap)`
    CodeMapsNothingNomap = "code-maps-nothing-nomap",
    /// `Remove this code`
    RemoveThisCode = "remove-this-code",
    /// `Preview this code through $translate`
    PreviewCodeThroughTranslate = "preview-code-through-translate",
    /// `A code not chosen yet`
    CodeNotChosenYet = "code-not-chosen-yet",
    /// `source code`
    SourceCode2 = "source-code-2",
    /// `Codes`
    Codes = "codes",
    /// `Target system version`
    TargetSystemVersion = "target-system-version",
    /// `Target system`
    TargetSystem = "target-system",
    /// `Source system version`
    SourceSystemVersion = "source-system-version",
    /// `Source system`
    SourceSystem = "source-system",
    /// `Remove this group`
    RemoveThisGroup = "remove-this-group",
    /// `Add a code`
    AddACode = "add-a-code",
    /// `A group with no systems yet`
    GroupNoSystemsYet = "group-no-systems-yet",
    /// `Add a group`
    AddAGroup = "add-a-group",
    /// `A group maps codes of one system to codes of another, and carries at least one code.`
    GroupMapsCodesOneSystemCodes = "group-maps-codes-one-system-codes",
    /// `Publication status`
    PublicationStatus = "publication-status",
    /// `Business version`
    BusinessVersion = "business-version",
    /// `Canonical (url)`
    CanonicalUrl = "canonical-url",
    /// `Metadata`
    Metadata = "metadata",
    /// `Read only.`
    ReadOnly = "read-only",
    /// `This account carries no permission to change a concept map. Sign in with one that does.`
    AccountCarriesNoPermissionChangeConcept = "account-carries-no-permission-change-concept",
    /// `The server states no version for this resource, so it is one this deployment loaded...`
    ServerStatesNoVersionResourceSo = "server-states-no-version-resource-so",
    /// `Its metadata, the systems each group maps between, and every code with what it maps to.`
    MetadataSystemsEachGroupMapsBetween = "metadata-systems-each-group-maps-between",
    /// `Translate`
    Translate = "translate",
    /// `Concept map`
    ConceptMap = "concept-map",
    /// `Concept maps this root holds`
    ConceptMapsRootHolds = "concept-maps-root-holds",
    /// `Concept map pages`
    ConceptMapPages = "concept-map-pages",
    /// `concept maps`
    ConceptMaps2 = "concept-maps-2",
    /// `This root holds no ConceptMap resource matching the filter above. The translate runner...`
    RootHoldsNoConceptmapResourceMatching = "root-holds-no-conceptmap-resource-matching",
    /// `Reading the concept maps`
    ReadingTheConceptMaps = "reading-the-concept-maps",
    /// `What this root holds`
    WhatThisRootHolds = "what-this-root-holds",
    /// `The url search parameter, matched against ConceptMap.url.`
    UrlSearchParameterMatchedAgainstConceptmap = "url-search-parameter-matched-against-conceptmap",
    /// `The ConceptMap resources this server publishes. Open one to see what it maps.`
    ConceptmapResourcesServerPublishesOpenOne = "conceptmap-resources-server-publishes-open-one",
    /// `Save this code system`
    SaveThisCodeSystem = "save-this-code-system",
    /// `Create this code system`
    CreateThisCodeSystem = "create-this-code-system",
    /// `A code system needs a canonical, a status, and a content mode before it can be saved.`
    CodeSystemNeedsCanonicalStatusContent = "code-system-needs-canonical-status-content",
    /// `Remove`
    Remove = "remove",
    /// `Add a property value`
    AddAPropertyValue = "add-a-property-value",
    /// `Property values`
    PropertyValues = "property-values",
    /// `Use (system|code)`
    UseSystemCode = "use-system-code",
    /// `Add a designation`
    AddADesignation = "add-a-designation",
    /// `On`
    On = "on",
    /// `Lifecycle`
    Lifecycle = "lifecycle",
    /// `Retire`
    Retire = "retire",
    /// `A concept with no code yet`
    ConceptNoCodeYet = "concept-no-code-yet",
    /// `Add a concept`
    AddAConcept = "add-a-concept",
    /// `A concept is retired rather than removed, so a code that has been published keeps...`
    ConceptRetiredRatherThanRemovedSo = "concept-retired-rather-than-removed-so",
    /// `Concepts`
    Concepts = "concepts",
    /// `Property type`
    PropertyType = "property-type",
    /// `Property identifier`
    PropertyIdentifier = "property-identifier",
    /// `Property code`
    PropertyCode = "property-code",
    /// `Declare a property`
    DeclareAProperty = "declare-a-property",
    /// `Type`
    Type = "type",
    /// `Identifier (uri)`
    IdentifierUri = "identifier-uri",
    /// `The lifecycle properties are declared by the editor on every save, so they are not...`
    LifecyclePropertiesDeclaredEditorEverySave = "lifecycle-properties-declared-editor-every-save",
    /// `Declared properties`
    DeclaredProperties = "declared-properties",
    /// `Codes are case sensitive`
    CodesAreCaseSensitive = "codes-are-case-sensitive",
    /// `Content mode`
    ContentMode = "content-mode",
    /// `the server stated none`
    TheServerStatedNone = "the-server-stated-none",
    /// `Parameter`
    Parameter = "parameter",
    /// `CodeSystem/$validate-code, run against the version that was just saved.`
    CodesystemValidateCodeRunAgainstVersion = "codesystem-validate-code-run-against-version",
    /// `What the server says about the retired concept`
    WhatServerSaysAboutRetiredConcept = "what-server-says-about-retired-concept",
    /// `This account carries no permission to change a code system. Sign in with one that does.`
    AccountCarriesNoPermissionChangeCode = "account-carries-no-permission-change-code",
    /// `The server states no version for this resource, so there is nothing an update could...`
    ServerStatesNoVersionResourceSo2 = "server-states-no-version-resource-so-2",
    /// `This code system is served from a built index, which has no write path. It is shown as...`
    CodeSystemServedFromBuiltIndex = "code-system-served-from-built-index",
    /// `This server's terminology capabilities did not read, so whether an artifact backs this...`
    ServerSTerminologyCapabilitiesDidNot = "server-s-terminology-capabilities-did-not",
    /// `Reading what this server can do with this code system.`
    ReadingWhatServerCanDoCode = "reading-what-server-can-do-code",
    /// `Reading the code system`
    ReadingTheCodeSystem = "reading-the-code-system",
    /// `Its metadata, the properties it declares, and its concepts with their designations and...`
    MetadataPropertiesDeclaresConceptsTheirDesignations = "metadata-properties-declares-concepts-their-designations",
    /// `Retired`
    Retired = "retired",
    /// `Deprecated`
    Deprecated = "deprecated",
    /// `Active`
    Active = "active",
    /// `What the system answered, in milliseconds`
    WhatSystemAnsweredMilliseconds = "what-system-answered-milliseconds",
    /// `Warm requests`
    WarmRequests = "warm-requests",
    /// `99th`
    N99th = "n-99th",
    /// `95th`
    N95th = "n-95th",
    /// `Median`
    Median = "median",
    /// `Cold`
    Cold = "cold",
    /// `No timing`
    NoTiming = "no-timing",
    /// `This record timed no operation.`
    RecordTimedNoOperation = "record-timed-no-operation",
    /// `Record`
    Record = "record",
    /// `Taken at`
    TakenAt = "taken-at",
    /// `Machine`
    Machine = "machine",
    /// `Answered by`
    AnsweredBy = "answered-by",
    /// `Served as`
    ServedAs = "served-as",
    /// `Resident memory`
    ResidentMemory = "resident-memory",
    /// `Ready after`
    ReadyAfter = "ready-after",
    /// `Built from`
    BuiltFrom = "built-from",
    /// `Offline build`
    OfflineBuild = "offline-build",
    /// `built outside this run`
    BuiltOutsideThisRun = "built-outside-this-run",
    /// `One record per code system the run loaded`
    OneRecordPerCodeSystemRun = "one-record-per-code-system-run",
    /// `The newest benchmark run`
    TheNewestBenchmarkRun = "the-newest-benchmark-run",
    /// `From`
    From2 = "from-2",
    /// `Each benchmark, its bar, and the run recorded against it`
    EachBenchmarkBarRunRecordedAgainst = "each-benchmark-bar-run-recorded-against",
    /// `The latency the project claims`
    LatencyProjectClaims = "latency-project-claims",
    /// . A bar is drawn against the slowest measurement beside it.
    BarDrawnAgainstSlowestMeasurementBeside = "bar-drawn-against-slowest-measurement-beside",
    /// `Recorded on`
    RecordedOn = "recorded-on",
    /// `What the bar claims`
    WhatTheBarClaims = "what-the-bar-claims",
    /// `Room`
    Room = "room",
    /// `Measured`
    Measured = "measured",
    ///  µs
    S = "s",
    /// `Bar`
    Bar = "bar",
    /// `faster than the run could time`
    FasterThanRunCouldTime = "faster-than-run-could-time",
    /// `The cases each mode passes, from the committed pass lists`
    CasesEachModePassesFromCommitted = "cases-each-mode-passes-from-committed",
    /// `The HL7 terminology ecosystem suite`
    Hl7TerminologyEcosystemSuite = "hl7-terminology-ecosystem-suite",
    ///  and
    And2 = "and-2",
    /// `The cases each mode passes`
    CasesEachModePasses = "cases-each-mode-passes",
    /// `Share`
    Share = "share",
    ///  of
    Of = "of",
    /// `Passed`
    Passed = "passed",
    /// `Surface`
    Surface = "surface",
    /// `, not from the server answering this page. Each figure names its file.`
    NotFromServerAnsweringPageEach = "not-from-server-answering-page-each",
    /// `Read from files this repository committed for FerroTERM`
    ReadFromFilesRepositoryCommittedFerroterm = "read-from-files-repository-committed-ferroterm",
    /// `Code systems in the newest run`
    CodeSystemsNewestRun = "code-systems-newest-run",
    /// `Latency bars the project claims`
    LatencyBarsProjectClaims = "latency-bars-project-claims",
    /// `Ecosystem suite cases pass`
    EcosystemSuiteCasesPass = "ecosystem-suite-cases-pass",
    /// `Operation`
    Operation = "operation",
    /// `Benchmark`
    Benchmark = "benchmark",
    /// `Mode`
    Mode = "mode",
    /// `The server refused to expand a selection this large in one answer. Ask for a page size...`
    ServerRefusedExpandSelectionLargeOne = "server-refused-expand-selection-large-one",
    /// `The parameters the server says it applied`
    ParametersServerSaysApplied = "parameters-server-says-applied",
    /// `The server echoed no parameters with this expansion.`
    ServerEchoedNoParametersExpansion = "server-echoed-no-parameters-expansion",
    /// `Expansion pages`
    ExpansionPages = "expansion-pages",
    /// `Flags`
    Flags = "flags",
    /// `The concepts of this page`
    ConceptsPage = "concepts-page",
    /// `A page past the end of a selection is empty, and so is a filter nothing matches.`
    PagePastEndSelectionEmptySo = "page-past-end-selection-empty-so",
    /// `No concept on this page`
    NoConceptPage = "no-concept-page",
    /// `this value set admits codes the expansion does not list, so a code missing from the...`
    ValueSetAdmitsCodesExpansionDoes = "value-set-admits-codes-expansion-does",
    /// `Unclosed expansion:`
    UnclosedExpansion = "unclosed-expansion",
    /// `The server stated no reason for it.`
    ServerStatedNoReason = "server-stated-no-reason",
    /// `The server answered a ValueSet carrying no expansion, so there is nothing to page through.`
    ServerAnsweredValuesetCarryingNoExpansion = "server-answered-valueset-carrying-no-expansion",
    /// `No expansion`
    NoExpansion = "no-expansion",
    /// `Name a value set above and run it. The canonical is sent exactly as you type it, so an...`
    NameValueSetAboveRunCanonical = "name-value-set-above-run-canonical",
    /// `No expansion yet`
    NoExpansionYet = "no-expansion-yet",
    /// `Running the expansion`
    RunningTheExpansion = "running-the-expansion",
    /// `The expansion`
    TheExpansion = "the-expansion",
    /// `Answer`
    Answer = "answer",
    /// `Sends includeDesignations=true, which asks for every designation the server holds for...`
    SendsIncludedesignationsTrueWhichAsksEvery = "sends-includedesignations-true-which-asks-every",
    /// `Include designations`
    IncludeDesignations = "include-designations",
    /// `The count parameter. Left empty, the run asks for no page and the server may refuse a...`
    CountParameterLeftEmptyRunAsks = "count-parameter-left-empty-run-asks",
    /// `Page size`
    PageSize = "page-size",
    /// `Selection`
    Selection = "selection",
    /// `Sends activeOnly=true, which asks the server to leave inactive concepts out of the...`
    SendsActiveonlyTrueWhichAsksServer = "sends-activeonly-true-which-asks-server",
    /// `Active concepts only`
    ActiveConceptsOnly = "active-concepts-only",
    /// `A BCP 47 tag. Left empty, the server picks its own display.`
    Bcp47TagLeftEmptyServer = "bcp-47-tag-left-empty-server",
    /// `Text the server matches against the designations it holds.`
    TextServerMatchesAgainstDesignationsHolds = "text-server-matches-against-designations-holds",
    /// `The value set`
    TheValueSet = "the-value-set",
    /// `The url parameter of $expand. The list is what this root publishes; an implicit...`
    UrlParameterExpandListWhatRoot = "url-parameter-expand-list-what-root",
    /// `Value set`
    ValueSet = "value-set",
    /// `Run the expansion`
    RunTheExpansion = "run-the-expansion",
    /// `No concepts on this page.`
    NoConceptsPage = "no-concepts-page",
    /// `Which codes a value set holds, from this server's $expand, a page at a time.`
    WhichCodesValueSetHoldsFrom = "which-codes-value-set-holds-from",
    /// `This root declares none of the operations that would apply to what you typed.`
    RootDeclaresNoneOperationsWouldApply = "root-declares-none-operations-would-apply",
    /// `Nowhere to go`
    NowhereToGo = "nowhere-to-go",
    /// `Reading what this root declares`
    ReadingWhatRootDeclares = "reading-what-root-declares",
    /// `Where it goes`
    WhereItGoes = "where-it-goes",
    /// `What you typed`
    WhatYouTyped = "what-you-typed",
    /// `What to do with it`
    WhatDo = "what-do",
    /// `Nothing was typed.`
    NothingWasTyped = "nothing-was-typed",
    /// `Release`
    Release = "release",
    /// `What the release did`
    WhatTheReleaseDid = "what-the-release-did",
    /// `Codes this resource names that the release the sync service last activated changed.`
    CodesResourceNamesReleaseSyncService = "codes-resource-names-release-sync-service",
    /// `What the last synchronisation found`
    WhatLastSynchronisationFound = "what-last-synchronisation-found",
    /// `What changed`
    WhatChanged = "what-changed",
    /// `Later`
    Later = "later",
    /// `Earlier`
    Earlier = "earlier",
    /// `Change`
    Change = "change",
    /// `Element`
    Element = "element",
    /// `Choose a version`
    ChooseAVersion = "choose-a-version",
    /// `Compare two versions`
    CompareTwoVersions = "compare-two-versions",
    /// `Compare these versions`
    CompareTheseVersions = "compare-these-versions",
    /// `Later version`
    LaterVersion = "later-version",
    /// `Earlier version`
    EarlierVersion = "earlier-version",
    /// `The server states nothing here.`
    ServerStatesNothingHere = "server-states-nothing-here",
    /// `This version is the one the server holds now, or this account cannot write this resource.`
    VersionOneServerHoldsNowAccount = "version-one-server-holds-now-account",
    /// `current`
    Current = "current",
    /// `Only the most recent versions are listed.`
    OnlyMostRecentVersionsListed = "only-most-recent-versions-listed",
    /// `This server declares the version read and not the history interaction, so the list is...`
    ServerDeclaresVersionReadNotHistory = "server-declares-version-read-not-history",
    /// `Restore`
    Restore = "restore",
    /// `Interaction`
    Interaction = "interaction",
    /// `Changed`
    Changed = "changed",
    /// `Restoring`
    Restoring = "restoring",
    /// `This version carries no resource to put back.`
    VersionCarriesNoResourcePutBack = "version-carries-no-resource-put-back",
    /// `The address names no resource type, so there is nothing to read the versions of.`
    AddressNamesNoResourceTypeSo = "address-names-no-resource-type-so",
    /// `Every version the server holds, what any two of them differ by, and putting an earlier...`
    EveryVersionServerHoldsWhatAny = "every-version-server-holds-what-any",
    /// `Reading the versions`
    ReadingTheVersions = "reading-the-versions",
    /// `The server answered no property.`
    ServerAnsweredNoProperty = "server-answered-no-property",
    /// `No property`
    NoProperty = "no-property",
    /// `The server answered no designation.`
    ServerAnsweredNoDesignation = "server-answered-no-designation",
    /// `No designation`
    NoDesignation = "no-designation",
    /// `Relatives of this concept`
    RelativesOfThisConcept = "relatives-of-this-concept",
    /// `The server answered no display`
    ServerAnsweredNoDisplay = "server-answered-no-display",
    /// `Looking the code up`
    LookingTheCodeUp = "looking-the-code-up",
    /// `Choose a code system, type a code and look it up.`
    ChooseCodeSystemTypeCodeLook = "choose-code-system-type-code-look",
    /// `No code read yet`
    NoCodeReadYet = "no-code-read-yet",
    /// `The code system`
    TheCodeSystem = "the-code-system",
    /// `The version parameter. Left on the first choice, the server answers from the version...`
    VersionParameterLeftFirstChoiceServer = "version-parameter-left-first-choice-server",
    /// `The system parameter of $lookup. The list is what this root publishes.`
    SystemParameterLookupListWhatRoot = "system-parameter-lookup-list-what-root",
    /// `Look up`
    LookUp = "look-up",
    /// `The code to read, exactly as the system writes it.`
    CodeReadExactlySystemWrites = "code-read-exactly-system-writes",
    /// `The code`
    TheCode = "the-code",
    /// `A code's display, designations and properties, from this server's $lookup.`
    CodeSDisplayDesignationsPropertiesFrom = "code-s-display-designations-properties-from",
    /// `Designation`
    Designation = "designation",
    /// `Run an operation`
    RunAnOperation = "run-an-operation",
    /// `Recent changes`
    RecentChanges = "recent-changes",
    /// `Writes per day`
    WritesPerDay = "writes-per-day",
    /// `Every resource on this root was loaded by the offline build.`
    EveryResourceRootWasLoadedOffline = "every-resource-root-was-loaded-offline",
    /// `Nothing written yet`
    NothingWrittenYet = "nothing-written-yet",
    /// `Reading what was written lately`
    ReadingWhatWasWrittenLately = "reading-what-was-written-lately",
    /// `This root serves none. A deployment loads one with the offline build.`
    RootServesNoneDeploymentLoadsOne = "root-serves-none-deployment-loads-one",
    /// `No code system`
    NoCodeSystem = "no-code-system",
    /// `All code systems`
    AllCodeSystems = "all-code-systems",
    /// `Reading what this server serves`
    ReadingWhatServerServes = "reading-what-server-serves",
    /// `Recent writes`
    RecentWrites = "recent-writes",
    /// `Follow a code through the concept maps this server holds.`
    FollowCodeThroughConceptMapsServer = "follow-code-through-concept-maps-server",
    /// `List the codes a value set holds, a page at a time.`
    ListCodesValueSetHoldsPage = "list-codes-value-set-holds-page",
    /// `Ask whether a code is in a code system or a value set.`
    AskWhetherCodeCodeSystemValue = "ask-whether-code-code-system-value",
    /// `Read a code's display, designations and properties in its system.`
    ReadCodeSDisplayDesignationsProperties = "read-code-s-display-designations-properties",
    /// `Preferences`
    Preferences = "preferences",
    /// `Save the settings`
    SaveTheSettings = "save-the-settings",
    /// `Kept by this console for this browser, in its own cookies.`
    KeptConsoleBrowserOwnCookies = "kept-console-browser-own-cookies",
    /// `How many rows a paged screen asks for, from 1 to 1000.`
    HowManyRowsPagedScreenAsks = "how-many-rows-paged-screen-asks",
    /// `for example nl-NL`
    ForExampleNlNl = "for-example-nl-nl",
    /// `A BCP 47 tag sent as displayLanguage. Leave it empty to take the server default.`
    Bcp47TagSentDisplaylanguageLeave = "bcp-47-tag-sent-displaylanguage-leave",
    /// `Used when an address carries no version of its own.`
    UsedWhenAddressCarriesNoVersion = "used-when-address-carries-no-version",
    /// `Default FHIR version`
    DefaultFhirVersion = "default-fhir-version",
    /// `Compact tightens every space and row, so more of a listing fits one screen.`
    CompactTightensEverySpaceRowSo = "compact-tightens-every-space-row-so",
    /// `Density`
    Density = "density",
    /// `System follows the colour scheme this device asks for.`
    SystemFollowsColourSchemeDeviceAsks = "system-follows-colour-scheme-device-asks",
    /// `Theme`
    Theme = "theme",
    /// `FHIR base in use`
    FhirBaseInUse = "fhir-base-in-use",
    /// `Filter by name or canonical`
    FilterNameCanonical = "filter-name-canonical",
    /// `1 match.`
    N1Match = "n-1-match",
    /// `No matches.`
    NoMatches = "no-matches",
    /// `The maps the server used`
    MapsServerUsed = "maps-server-used",
    /// `a value shape this viewer draws no text for`
    ValueShapeViewerDrawsNoText = "value-shape-viewer-draws-no-text",
    /// `Depends on`
    DependsOn = "depends-on",
    /// `Also produces`
    AlsoProduces = "also-produces",
    /// `What came with it`
    WhatCameWithIt = "what-came-with-it",
    /// `Target comment`
    TargetComment = "target-comment",
    /// `Source comment`
    SourceComment = "source-comment",
    /// `From map`
    FromMap = "from-map",
    /// `From concept`
    FromConcept = "from-concept",
    /// `none stated`
    NoneStated = "none-stated",
    /// `This code maps to nothing in the target`
    CodeMapsNothingTarget = "code-maps-nothing-target",
    /// `The matches the server reported`
    MatchesServerReported = "matches-server-reported",
    /// `The server reported no match for this code.`
    ServerReportedNoMatchCode = "server-reported-no-match-code",
    /// `The server did not translate the code.`
    ServerDidNotTranslateCode = "server-did-not-translate-code",
    /// `Running the translation`
    RunningTheTranslation = "running-the-translation",
    /// `The translation`
    TheTranslation = "the-translation",
    /// `Name a code and the system it belongs to, then translate it.`
    NameCodeSystemBelongsThenTranslate = "name-code-system-belongs-then-translate",
    /// `Nothing translated yet`
    NothingTranslatedYet = "nothing-translated-yet",
    /// `Narrows the answer to matches in this system. The list is what this root publishes....`
    NarrowsAnswerMatchesSystemListWhat = "narrows-answer-matches-system-list-what",
    /// `Target code system`
    TargetCodeSystem = "target-code-system",
    /// `The target`
    TheTarget = "the-target",
    /// `The code to translate, sent exactly as you type it. The operation needs this and the...`
    CodeTranslateSentExactlyYouType = "code-translate-sent-exactly-you-type",
    /// `The list is the versions this root published of the code system named beside it. Left...`
    ListVersionsRootPublishedCodeSystem = "list-versions-root-published-code-system",
    /// `Code system version`
    CodeSystemVersion = "code-system-version",
    /// `The system the code belongs to. The list is what this root publishes. The operation...`
    SystemCodeBelongsListWhatRoot = "system-code-belongs-list-what-root",
    /// `Sent as conceptMapVersion. The list is the versions this root published of the map...`
    SentConceptmapversionListVersionsRootPublished = "sent-conceptmapversion-list-versions-root-published",
    /// `Concept map version`
    ConceptMapVersion = "concept-map-version",
    /// `Sent as the url parameter. The list is what this root publishes. Left empty, the...`
    SentUrlParameterListWhatRoot = "sent-url-parameter-list-what-root",
    /// `The map`
    TheMap = "the-map",
    /// `This root's capability statement does not declare $translate on ConceptMap, so the...`
    RootSCapabilityStatementDoesNot = "root-s-capability-statement-does-not",
    /// `Reading what this root can do`
    ReadingWhatRootCanDo = "reading-what-root-can-do",
    /// `One code through the concept maps this server holds, from its $translate.`
    OneCodeThroughConceptMapsServer = "one-code-through-concept-maps-server",
    /// `Target`
    Target = "target",
    /// `Nothing run yet`
    NothingRunYet = "nothing-run-yet",
    /// `The server answered no outcome.`
    ServerAnsweredNoOutcome = "server-answered-no-outcome",
    /// `The server answered no outcome parameter, so there is nothing to report about these...`
    ServerAnsweredNoOutcomeParameterSo = "server-answered-no-outcome-parameter-so",
    /// `The issues the server itemised`
    IssuesServerItemised = "issues-server-itemised",
    /// `Code systems this server does not hold`
    CodeSystemsServerDoesNotHold = "code-systems-server-does-not-hold",
    /// `Text:`
    Text = "text",
    /// `The concept the server echoed`
    ConceptServerEchoed = "concept-server-echoed",
    /// `This concept is inactive in its code system. The server served it and marked it, which...`
    ConceptInactiveCodeSystemServerServed = "concept-inactive-code-system-server-served",
    /// `Inactive`
    Inactive2 = "inactive-2",
    /// `Version validated in`
    VersionValidatedIn = "version-validated-in",
    /// `Code as the system spells it`
    CodeSystemSpells = "code-system-spells",
    /// `The server declared no result`
    ServerDeclaredNoResult = "server-declared-no-result",
    /// `Not valid: result false`
    NotValidResultFalse = "not-valid-result-false",
    /// `Valid: result true`
    ValidResultTrue = "valid-result-true",
    /// `How do two codes relate?`
    HowDoTwoCodesRelate = "how-do-two-codes-relate",
    /// `Run the subsumption test`
    RunTheSubsumptionTest = "run-the-subsumption-test",
    /// `The second code. Both codes are read in the code system named above.`
    SecondCodeBothCodesReadCode = "second-code-both-codes-read-code",
    /// `Code B`
    CodeB = "code-b",
    /// `The first code. The answer says how it relates to code B.`
    FirstCodeAnswerSaysHowRelates = "first-code-answer-says-how-relates",
    /// `Code A`
    CodeA = "code-a",
    /// `The two codes`
    TheTwoCodes = "the-two-codes",
    /// `This root declares the instance level of $subsumes, so a stored CodeSystem's id can...`
    RootDeclaresInstanceLevelSubsumesSo = "root-declares-instance-level-subsumes-so",
    /// `CodeSystem id (instance level)`
    CodesystemIdInstanceLevel = "codesystem-id-instance-level",
    /// `The runner offers $subsumes only for a code system whose terminology capabilities...`
    RunnerOffersSubsumesOnlyCodeSystem = "runner-offers-subsumes-only-code-system",
    /// `This root's capability statement does not declare $subsumes on CodeSystem, so the...`
    RootSCapabilityStatementDoesNot2 = "root-s-capability-statement-does-not-2",
    /// `Name two codes of this system and run them.`
    NameTwoCodesSystemRunThem = "name-two-codes-system-run-them",
    /// `Running the subsumption test`
    RunningTheSubsumptionTest = "running-the-subsumption-test",
    /// `The relation`
    TheRelation = "the-relation",
    /// `Only the resource types this root's capability statement declares $validate-code on...`
    OnlyResourceTypesRootSCapability = "only-resource-types-root-s-capability",
    /// `Validate against`
    ValidateAgainst = "validate-against",
    /// `A value set (ValueSet/$validate-code)`
    ValueSetValuesetValidateCode = "value-set-valueset-validate-code",
    /// `A code system (CodeSystem/$validate-code)`
    CodeSystemCodesystemValidateCode = "code-system-codesystem-validate-code",
    /// `The code to check. Nothing is sent until this is filled in.`
    CodeCheckNothingSentUntilFilled = "code-check-nothing-sent-until-filled",
    /// `The list is the versions this root published of the code system named beside it. Left...`
    ListVersionsRootPublishedCodeSystem2 = "list-versions-root-published-code-system-2",
    /// `The code system the code belongs to. The list is what this root publishes, and a...`
    CodeSystemCodeBelongsListWhat = "code-system-code-belongs-list-what",
    /// `What the code is checked against`
    WhatCodeCheckedAgainst = "what-code-checked-against",
    /// `This root declares the instance level of $validate-code, so a stored resource's id can...`
    RootDeclaresInstanceLevelValidateCode = "root-declares-instance-level-validate-code",
    /// `Resource id (instance level)`
    ResourceIdInstanceLevel = "resource-id-instance-level",
    /// `The valueSetVersion parameter. The list is the versions this root published of the...`
    ValuesetversionParameterListVersionsRootPublished = "valuesetversion-parameter-list-versions-root-published",
    /// `Value set version`
    ValueSetVersion = "value-set-version",
    /// `The url parameter of ValueSet/$validate-code. The list is what this root publishes; an...`
    UrlParameterValuesetValidateCodeList = "url-parameter-valueset-validate-code-list",
    /// `Is this code valid?`
    IsThisCodeValid = "is-this-code-valid",
    /// `Run the validation`
    RunTheValidation = "run-the-validation",
    /// `This root's capability statement does not declare $validate-code on the resource type...`
    RootSCapabilityStatementDoesNot3 = "root-s-capability-statement-does-not-3",
    /// `Name a code and the resource to check it against, then run it.`
    NameCodeResourceCheckAgainstThen = "name-code-resource-check-against-then",
    /// `Running the validation`
    RunningTheValidation = "running-the-validation",
    /// `The validation`
    TheValidation = "the-validation",
    /// `It declares no hierarchy filter operator, so its concepts are a flat list.`
    DeclaresNoHierarchyFilterOperatorSo = "declares-no-hierarchy-filter-operator-so",
    /// `This root declares nothing about subsumption for this system, so no $subsumes form is...`
    RootDeclaresNothingAboutSubsumptionSystem = "root-declares-nothing-about-subsumption-system",
    /// `This system declares that it does not answer subsumption, so no $subsumes form is...`
    SystemDeclaresDoesNotAnswerSubsumption = "system-declares-does-not-answer-subsumption",
    /// `This system declares subsumption, so the $subsumes panel below runs against it.`
    SystemDeclaresSubsumptionSoSubsumesPanel = "system-declares-subsumption-so-subsumes-panel",
    /// `No code system is named yet. Choose one above, name the code, and run it.`
    NoCodeSystemNamedYetChoose = "no-code-system-named-yet-choose",
    /// `The code system this screen works over`
    CodeSystemScreenWorksOver = "code-system-screen-works-over",
    /// `Whether a code is valid in a code system or a value set, from this server's...`
    WhetherCodeValidCodeSystemValue = "whether-code-valid-code-system-value",
    /// `The ValueSet, as the server sent it`
    ValuesetServerSent = "valueset-server-sent",
    /// `Reading the value set`
    ReadingTheValueSet = "reading-the-value-set",
    /// `The codes the value set expands to`
    CodesValueSetExpands = "codes-value-set-expands",
    /// `Open in the runner`
    OpenInTheRunner = "open-in-the-runner",
    /// `No code`
    NoCode = "no-code",
    /// `The expansion lists no code.`
    ExpansionListsNoCode = "expansion-lists-no-code",
    /// `The server answered without an expansion.`
    ServerAnsweredWithoutExpansion = "server-answered-without-expansion",
    /// `Expanding the value set`
    ExpandingTheValueSet = "expanding-the-value-set",
    /// `This resource declares no canonical, so there is nothing to expand.`
    ResourceDeclaresNoCanonicalSoThere = "resource-declares-no-canonical-so-there",
    /// `No canonical`
    NoCanonical = "no-canonical",
    /// `the whole code system`
    TheWholeCodeSystem = "the-whole-code-system",
    /// `every code in`
    EveryCodeIn = "every-code-in",
    /// `1 code named one by one`
    N1CodeNamedOneOne = "n-1-code-named-one-one",
    /// `Selects`
    Selects = "selects",
    /// `What the definition selects`
    WhatTheDefinitionSelects = "what-the-definition-selects",
    /// `Its content is whatever the server holds for it, not a selection written here.`
    ContentWhateverServerHoldsNotSelection = "content-whatever-server-holds-not-selection",
    /// `No compose element`
    NoComposeElement = "no-compose-element",
    /// `Clauses`
    Clauses = "clauses",
    /// `Compose`
    Compose = "compose",
    /// `Views of this value set`
    ViewsValueSet = "views-value-set",
    /// `Clause`
    Clause = "clause",
    /// `Expansion`
    Expansion = "expansion",
    /// `The server refused the preview.`
    ServerRefusedPreview = "server-refused-preview",
    /// `The server answered no expansion for this definition.`
    ServerAnsweredNoExpansionDefinition = "server-answered-no-expansion-definition",
    /// `Next`
    Next = "next",
    /// `Previous`
    Previous = "previous",
    /// `The server answered a ValueSet carrying no expansion.`
    ServerAnsweredValuesetCarryingNoExpansion2 = "server-answered-valueset-carrying-no-expansion-2",
    /// `Run the preview`
    RunThePreview = "run-the-preview",
    /// `Filter the preview`
    FilterThePreview = "filter-the-preview",
    /// `Expanding the definition`
    ExpandingTheDefinition = "expanding-the-definition",
    /// `The preview`
    ThePreview = "the-preview",
    /// `Run the preview to see what this definition selects. It expands the definition on...`
    RunPreviewSeeWhatDefinitionSelects = "run-preview-see-what-definition-selects",
    /// `A canonical and a clause that selects something are what a save needs.`
    CanonicalClauseSelectsSomethingWhatSave = "canonical-clause-selects-something-what-save",
    /// `Save this value set`
    SaveThisValueSet = "save-this-value-set",
    /// `Saved.`
    Saved = "saved",
    /// `The value the operator is applied with.`
    ValueOperatorApplied = "value-operator-applied",
    /// `Filters`
    Filters = "filters",
    /// `This server declares no filter for that code system version.`
    ServerDeclaresNoFilterCodeSystem = "server-declares-no-filter-code-system",
    /// `Choose a property and an operator`
    ChoosePropertyOperator = "choose-property-operator",
    /// `compose.include.filter, over a property this code system version declares.`
    ComposeIncludeFilterOverPropertyCode = "compose-include-filter-over-property-code",
    /// `Add a filter this server declares`
    AddFilterServerDeclares = "add-filter-server-declares",
    /// `Add`
    Add = "add",
    /// `No concept of that code system matches that text.`
    NoConceptCodeSystemMatchesText = "no-concept-code-system-matches-text",
    /// `The server answered no expansion for that search.`
    ServerAnsweredNoExpansionSearch = "server-answered-no-expansion-search",
    /// `Searching that code system`
    SearchingThatCodeSystem = "searching-that-code-system",
    /// `Find the code for that template`
    FindCodeTemplate = "find-code-template",
    /// `Find a code in that code system`
    FindCodeCodeSystem = "find-code-code-system",
    /// `compose.include.concept.display, which overrides the code system's own.`
    ComposeIncludeConceptDisplayWhichOverrides = "compose-include-concept-display-which-overrides",
    /// `The display this value set gives that code`
    DisplayValueSetGivesCode = "display-value-set-gives-code",
    /// `compose.include.version. Left empty, the server resolves the version itself.`
    ComposeIncludeVersionLeftEmptyServer = "compose-include-version-left-empty-server",
    /// `Choose one this server serves`
    ChooseOneServerServes = "choose-one-server-serves",
    /// `compose.include.system, the code system the codes below are read from.`
    ComposeIncludeSystemCodeSystemCodes = "compose-include-system-code-system-codes",
    /// `What replaces that placeholder in the template.`
    WhatReplacesPlaceholderTemplate = "what-replaces-placeholder-template",
    /// `Choose a template`
    ChooseATemplate = "choose-a-template",
    /// `An implicit value set, named by a URL template the code system publishes.`
    ImplicitValueSetNamedUrlTemplate = "implicit-value-set-named-url-template",
    /// `Add a value set the code system defines`
    AddValueSetCodeSystemDefines = "add-value-set-code-system-defines",
    /// `compose.include.valueSet, which draws the whole of that value set in.`
    ComposeIncludeValuesetWhichDrawsWhole = "compose-include-valueset-which-draws-whole",
    /// `Add a published value set`
    AddPublishedValueSet = "add-published-value-set",
    /// `Remove this clause`
    RemoveThisClause = "remove-this-clause",
    /// `Include`
    Include = "include",
    /// `Exclude`
    Exclude = "exclude",
    /// `Add an include`
    AddAnInclude = "add-an-include",
    /// `Add an exclude`
    AddAnExclude = "add-an-exclude",
    /// `Every exclude takes codes back out of that union, whichever include drew them in.`
    EveryExcludeTakesCodesBackOut = "every-exclude-takes-codes-back-out",
    /// `Every include adds to the selection: the value set is the union of them all.`
    EveryIncludeAddsSelectionValueSet = "every-include-adds-selection-value-set",
    /// `Includes`
    Includes = "includes",
    /// `Excludes`
    Excludes = "excludes",
    /// `Leave them out`
    LeaveThemOut = "leave-them-out",
    /// `Include them`
    IncludeThem = "include-them",
    /// `Leave it to the server`
    LeaveServer = "leave-server",
    /// `compose.inactive. Left to the server, the expansion parameters decide.`
    ComposeInactiveLeftServerExpansionParameters = "compose-inactive-left-server-expansion-parameters",
    /// `Inactive codes`
    InactiveCodes = "inactive-codes",
    /// `Choose a status`
    ChooseAStatus = "choose-a-status",
    /// `ValueSet.status, which every definitional resource carries.`
    ValuesetStatusWhichEveryDefinitionalResource = "valueset-status-which-every-definitional-resource",
    /// `ValueSet.title, the name written for a person.`
    ValuesetTitleNameWrittenPerson = "valueset-title-name-written-person",
    /// `ValueSet.name, the computer-friendly name.`
    ValuesetNameComputerFriendlyName = "valueset-name-computer-friendly-name",
    /// `ValueSet.url, what everything else refers to this value set by.`
    ValuesetUrlWhatEverythingElseRefers = "valueset-url-what-everything-else-refers",
    /// `This value set has no write path on this server, or this account carries no permission...`
    ValueSetHasNoWritePath = "value-set-has-no-write-path",
    /// `Sign in to save. Composing and previewing work without one.`
    SignSaveComposingPreviewingWorkWithout = "sign-save-composing-previewing-work-without",
    /// `This account carries no permission to create a value set, so nothing here can be...`
    AccountCarriesNoPermissionCreateValue = "account-carries-no-permission-create-value",
    /// `Draw in a published value set, add your own codes, and see what the selection holds.`
    DrawPublishedValueSetAddYour = "draw-published-value-set-add-your",
    /// `Value sets this root holds`
    ValueSetsRootHolds = "value-sets-root-holds",
    /// `Value set pages`
    ValueSetPages = "value-set-pages",
    /// `value sets`
    ValueSets2 = "value-sets-2",
    /// `This root holds no ValueSet resource matching the filter above. A value set a code...`
    RootHoldsNoValuesetResourceMatching = "root-holds-no-valueset-resource-matching",
    /// `Reading the value sets`
    ReadingTheValueSets = "reading-the-value-sets",
    /// `The url search parameter, matched against ValueSet.url.`
    UrlSearchParameterMatchedAgainstValueset = "url-search-parameter-matched-against-valueset",
    /// `The ValueSet resources this server publishes. Open one to see what it selects.`
    ValuesetResourcesServerPublishesOpenOne = "valueset-resources-server-publishes-open-one",
    /// `1 operation with a note`
    N1OperationNote = "n-1-operation-note",
    /// `This root added nothing to the definitions its release publishes, or did not answer.`
    RootAddedNothingDefinitionsReleasePublishes = "root-added-nothing-definitions-release-publishes",
    /// `What each root adds to its own definition`
    WhatEachRootAddsOwnDefinition = "what-each-root-adds-own-definition",
    /// `One root's own operation.documentation, as it sent it.`
    OneRootSOwnOperationDocumentation = "one-root-s-own-operation-documentation",
    /// `no answer`
    NoAnswer = "no-answer",
    /// `Operations, and the levels each answers at`
    OperationsLevelsEachAnswers = "operations-levels-each-answers",
    /// `No root that answered declares an operation.`
    NoRootAnsweredDeclaresOperation = "no-root-answered-declares-operation",
    /// `No operation`
    NoOperation = "no-operation",
    ///  did not answer
    DidNotAnswer = "did-not-answer",
    /// `What each root says about itself`
    WhatEachRootSaysAboutItself = "what-each-root-says-about-itself",
    /// `Declaration`
    Declaration = "declaration",
    /// `Reading the four capability statements`
    ReadingFourCapabilityStatements = "reading-four-capability-statements",
    /// `What each root declares`
    WhatEachRootDeclares = "what-each-root-declares",
    /// `Back to the concept map`
    BackConceptMap = "back-concept-map",
    /// `Back to the value set`
    BackValueSet = "back-value-set",
    /// `Back to the code system`
    BackCodeSystem = "back-code-system",
    /// `the display language`
    TheDisplayLanguage = "the-display-language",
    /// The target ``$code`` of ``$element`` is ``$relationship``, and this FHIR version...
    TargetFhirVersionRequiresCommentTarget = "target-fhir-version-requires-comment-target",
    /// The target ``$code`` of ``$element`` states no relationship to its source, which every...
    TargetStatesNoRelationshipSourceWhich = "target-states-no-relationship-source-which",
    /// `` `$element` is marked as mapping to nothing and carries a target as well. Untick that... ``
    MarkedMappingNothingCarriesTargetWell = "marked-mapping-nothing-carries-target-well",
    /// `$more elements differ.`
    ElementsDiffer = "elements-differ",
    /// `$answered of $answers roots answered`
    OfRootsAnswered = "of-roots-answered",
    /// Reloaded. The form now shows version `$version_id`, the one the server holds.
    ReloadedFormNowShowsVersionOne = "reloaded-form-now-shows-version-one",
    /// FHIR `$fhir`, `$software`
    Fhir2 = "fhir-2",
    /// FHIR `$fhir`
    Fhir3 = "fhir-3",
    /// is in every one of the `$several` value sets named here
    EveryOneValueSetsNamedHere = "every-one-value-sets-named-here",
    /// is one of the `$self` codes listed
    OneCodesListed = "one-codes-listed",
    /// matches every one of the `$self` filters
    MatchesEveryOneFilters = "matches-every-one-filters",
    /// `$verb nothing selects yet.`
    NothingSelectsYet = "nothing-selects-yet",
    /// the server answered a status of `$status`
    ServerAnsweredStatus = "server-answered-status",
    /// `$code_a and $code_b are the same concept.`
    SameConcept = "same-concept",
    /// Neither `$code_a` nor `$code_b` subsumes the other.
    NeitherNorSubsumesOther = "neither-nor-subsumes-other",
    /// The server answered the outcome ``$other``, which is not one of the four this viewer...
    ServerAnsweredOutcomeWhichNotOne = "server-answered-outcome-which-not-one",
    /// W/"`$version_id`"
    W = "w",
    /// `$shown This root counted $not_declared.`
    ThisRootCounted = "this-root-counted",
    /// `$shown This root counted $total and sent $carried, so this is part of the answer.`
    RootCountedSentSoPartAnswer = "root-counted-sent-so-part-answer",
    /// `$shown This root counted $total.`
    ThisRootCounted2 = "this-root-counted-2",
    /// No `$noun` on this page.
    NoOnThisPage = "no-on-this-page",
    /// Showing `$noun` `$first` to `$last` of `$matched`.
    ShowingToOf = "showing-to-of",
    /// `Version $not_declared`
    Version2 = "version-2",
    /// The note that the version declares the `child-of` operator on a filter.
    VersionDeclaresOperatorFilterSoTree = "version-declares-operator-filter-so-tree",
    /// This version declares the hierarchy operators `$operators`, and none of them selects...
    VersionDeclaresHierarchyOperatorsNoneThem = "version-declares-hierarchy-operators-none-them",
    /// `$found concepts matched.`
    ConceptsMatched = "concepts-matched",
    /// `$found concepts, which is the whole page this screen asked for. Narrow the filter to...`
    ConceptsWhichWholePageScreenAsked = "concepts-which-whole-page-screen-asked",
    /// `Display $not_declared`
    Display2 = "display-2",
    /// `Display $not_declared`
    Display3 = "display-3",
    /// `Read from a version this server did not name, of the system $not_declared`
    ReadFromVersionServerDidNot = "read-from-version-server-did-not",
    /// Read from version `$version`
    ReadFromVersion = "read-from-version",
    /// `$label of this concept`
    OfThisConcept = "of-this-concept",
    /// `Display $not_declared`
    Display4 = "display-4",
    /// `$display (more children than this page size asked for)`
    MoreChildrenThanPageSizeAsked = "more-children-than-page-size-asked",
    /// `Version $not_declared`
    Version3 = "version-3",
    /// Version `$code`
    Version4 = "version-4",
    /// `Filter $not_declared`
    Filter2 = "filter-2",
    /// `this root counted $not_declared`
    ThisRootCounted3 = "this-root-counted-3",
    /// this root counted `$total`
    ThisRootCounted4 = "this-root-counted-4",
    /// 1 published resource, and `$counted`
    N1PublishedResourceAnd = "n-1-published-resource-and",
    /// `$drawn published resources, and $counted`
    PublishedResourcesAnd = "published-resources-and",
    /// `Version $not_declared`
    Version5 = "version-5",
    /// Version `$version`
    Version6 = "version-6",
    /// Concept map `$id`
    ConceptMap3 = "concept-map-3",
    /// `$all mappings`
    Mappings2 = "mappings-2",
    /// `$shown of $all mappings`
    OfMappings = "of-mappings",
    /// Source scope (`$value0`)
    SourceScope = "source-scope",
    /// Target scope (`$value0`)
    TargetScope = "target-scope",
    /// `$source to $target`
    To2 = "to-2",
    /// Relationship (`$value0`)
    Relationship2 = "relationship-2",
    /// Searching for `$asked`
    SearchingFor = "searching-for",
    /// The search for `$asked` did not answer.
    SearchDidNotAnswer = "search-did-not-answer",
    /// `$rows codes match $asked.`
    CodesMatch = "codes-match",
    /// Find a `$what` in this system
    FindSystem = "find-system",
    /// The map has to be complete before it can be translated through. `$first`
    MapHasCompleteBeforeCanTranslated = "map-has-complete-before-can-translated",
    /// The preview did not answer: `$value0`
    PreviewDidNotAnswer = "preview-did-not-answer",
    /// Translated through `$through`: `$answer` matches.
    TranslatedThroughMatches = "translated-through-matches",
    /// Saved. The server now holds version `$version_id`.
    SavedServerNowHoldsVersion = "saved-server-now-holds-version",
    /// Saved. The server now holds version `$version_id`.
    SavedServerNowHoldsVersion2 = "saved-server-now-holds-version-2",
    /// The check on ``$code`` did not answer: `$error`
    CheckDidNotAnswer = "check-did-not-answer",
    /// The server reports ``$code`` as inactive.
    ServerReportsInactive = "server-reports-inactive",
    /// The server reports ``$code`` as still active.
    ServerReportsStillActive = "server-reports-still-active",
    /// The server stated no inactive flag for ``$code``.
    ServerStatedNoInactiveFlag = "server-stated-no-inactive-flag",
    /// `$passed of $ran`
    Of3 = "of-3",
    /// `$times times under it`
    TimesUnderIt = "times-under-it",
    /// `$seconds s`
    S2 = "s-2",
    /// `$ready_seconds s`
    S3 = "s-3",
    /// No concepts on this page, of `$total` in the selection.
    NoConceptsPageSelection = "no-concepts-page-selection",
    /// Concepts `$first` to `$last` of `$total`.
    ConceptsToOf = "concepts-to-of",
    /// Concepts `$first` to `$last`. The server declared no total.
    ConceptsServerDeclaredNoTotal = "concepts-server-declared-no-total",
    /// Page `$self` of `$self2`
    PageOf2 = "page-of-2",
    /// Page `$self`
    Page = "page",
    /// Not used: the address asked for a page size of ``$refused``, which is not a whole...
    NotUsedAddressAskedPageSize = "not-used-address-asked-page-size",
    /// nested at level `$row`
    NestedAtLevel = "nested-at-level",
    /// `$rows concepts, of $total in the selection.`
    ConceptsSelection = "concepts-selection",
    /// `$rows concepts.`
    Concepts2 = "concepts-2",
    /// `$counted The answer nests, and $expand pages flat expansions only, so this run was not...`
    AnswerNestsExpandPagesFlatExpansions = "answer-nests-expand-pages-flat-expansions",
    /// Read as `$subject` from its shape alone.
    ReadFromShapeAlone = "read-from-shape-alone",
    /// This editor does not write `$resource_type`, so it reads no versions of one.
    EditorDoesNotWriteSoReads = "editor-does-not-write-so-reads",
    /// Restore version `$labelled`
    RestoreVersion = "restore-version",
    /// Restored version `$chosen`. The server stated no version for the write.
    RestoredVersionServerStatedNoVersion = "restored-version-server-stated-no-version",
    /// Restored version `$chosen`. The server now holds version `$now`.
    RestoredVersionServerNowHoldsVersion = "restored-version-server-now-holds-version",
    /// Version `$id`
    Version7 = "version-7",
    /// The code systems, value sets and concept maps this server serves on FHIR `$version`.
    CodeSystemsValueSetsConceptMaps = "code-systems-value-sets-concept-maps",
    /// `$changes writes from $first to $last`
    WritesFromTo = "writes-from-to",
    /// `Result $not_declared.`
    Result = "result",
    /// `Target $not_declared`
    Target2 = "target-2",
    /// `$matches matches.`
    Matches = "matches",
    /// `Version $not_declared`
    Version8 = "version-8",
    /// The note that the version declares the `child-of` operator, so it walks a hierarchy.
    DeclaresOperatorFilterSoHasHierarchy = "declares-operator-filter-so-has-hierarchy",
    /// It declares the hierarchy operators `$operators`, none of which selects the direct...
    DeclaresHierarchyOperatorsNoneWhichSelects = "declares-hierarchy-operators-none-which-selects",
    ///  (version `$version`)
    Version9 = "version-9",
    /// Value set `$id`
    ValueSet3 = "value-set-3",
    /// `$codes codes named one by one`
    CodesNamedOneOne = "codes-named-one-one",
    /// `$listed codes shown`
    CodesShown = "codes-shown",
    /// `$listed of $total codes`
    OfCodes = "of-codes",
    /// `$rows of $total concepts on this page.`
    ConceptsPage2 = "concepts-page-2",
    /// `$rows concepts on this page. The server declared no total.`
    ConceptsPageServerDeclaredNoTotal = "concepts-page-server-declared-no-total",
    /// `$form, at character $column`
    AtCharacter = "at-character",
    /// This definition selects `$total` concepts.
    ThisDefinitionSelectsConcepts = "this-definition-selects-concepts",
    /// `$expansion concepts on this page. The server declared no total.`
    ConceptsPageServerDeclaredNoTotal2 = "concepts-page-server-declared-no-total-2",
    /// The server refused the preview: `$said`
    ServerRefusedPreview2 = "server-refused-preview-2",
    /// `$column · FHIR $release, $state`
    Fhir4 = "fhir-4",
    /// `$count operations with a note`
    OperationsWithANote = "operations-with-a-note",
    /// A root that answered.
    StateAnswered = "state-answered",
    /// A root that refused.
    StateRefusedWord = "state-refused-word",
    /// An operation the server declares.
    StateDeclared = "state-declared",
    /// The noun of the code systems, by count, with its empty and opening forms.
    NounCodeSystems = "noun-code-systems",
    /// The filter's count when it keeps every one of the `$held` code systems.
    SystemsHeld = "systems-held",
    /// The filter's count: `$shown` of the `$held` code systems.
    SystemsKept = "systems-kept",
    /// A grammar the server read.
    StateRead = "state-read",
    /// The mark of the version a request with no version resolves to.
    DefaultMark = "default-mark",
    /// The noun of the concept maps, by count, with its empty and opening forms.
    NounConceptMaps = "noun-concept-maps",
    /// The noun of the value sets, by count, with its empty and opening forms.
    NounValueSets = "noun-value-sets",
    /// The noun of the concepts, by count, with its empty and opening forms.
    NounConcepts = "noun-concepts",

    /// The title of the code system editor: `$mode` new or edit, `$name` the canonical changed.
    TitleCodeSystemEditor = "title-code-system-editor",
    /// The title of the concept map editor: `$mode` new or edit, `$name` the canonical changed.
    TitleConceptMapEditor = "title-concept-map-editor",
    /// The title of the value set composer: `$mode` new or edit, `$name` the value set changed.
    TitleValueSetComposer = "title-value-set-composer",
    /// The empty properties list's heading.
    NoPropertiesTitle = "no-properties-title",
    /// The empty properties list's line.
    NoPropertiesLine = "no-properties-line",
    /// The empty concepts list's heading.
    NoConceptsTitle = "no-concepts-title",
    /// The empty concepts list's line.
    NoConceptsLine = "no-concepts-line",
    /// The empty groups list's heading.
    NoGroupsTitle = "no-groups-title",
    /// The empty groups list's line.
    NoGroupsLine = "no-groups-line",
    /// The empty list of a group's codes.
    NoElementsTitle = "no-elements-title",
    /// The line of the empty list of a group's codes.
    NoElementsLine = "no-elements-line",
    /// `$code_a subsumes $code_b: $code_b is a descendant of $code_a.`
    SubsumesDescendant = "subsumes-descendant",
    /// `$code_a is subsumed by $code_b: $code_a is a descendant of $code_b.`
    SubsumedDescendant = "subsumed-descendant",
    /// `Find`
    TitleFind = "title-find",
}

/// The console's message `msg` in the page's locale.
#[must_use]
pub(crate) fn tr(msg: Msg) -> String {
    tr_in(catalog(), &locale(), msg)
}

/// The console's message `msg` in the page's locale, with the placeables
/// `args` filled in.
#[must_use]
pub(crate) fn tr_with(msg: Msg, args: &[(&'static str, Arg)]) -> String {
    locale().lookup_with(catalog(), msg.id(), args)
}

/// The console's message `msg` as a [`Message`] a component looks up as it
/// draws, such as a listing's noun or an editor's title.
#[must_use]
pub(crate) fn message(msg: Msg) -> Message {
    Message::new(catalog(), msg.id())
}

/// The message `msg` of `catalog` in `locale`.
#[must_use]
pub(crate) fn tr_in(catalog: &dyn Loader, locale: &Locale, msg: Msg) -> String {
    locale.lookup(catalog, msg.id())
}

/// The console's message `msg` in the locale of the request being served:
/// for a server function, which runs outside the page tree.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub(crate) fn tr_served(msg: Msg) -> String {
    requested().lookup(catalog(), msg.id())
}

/// The locale of the request being served: the best language of its
/// `Accept-Language` the catalog holds, else the configured one, English
/// with no configuration in reach.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub(crate) fn requested() -> Locale {
    use leptos::context::use_context;
    let configured = use_context::<crate::server::ViewerState>()
        .map_or_else(Locale::english, |state| state.settings().locale.clone());
    let header = use_context::<http::request::Parts>()
        .and_then(|parts| {
            parts
                .headers
                .get(http::header::ACCEPT_LANGUAGE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    Locale::negotiate_with(&header, catalog(), configured)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::path::Path;

    use ferrohealth_ui::i18n::Locale;
    use fluent_templates::fluent_bundle::FluentValue;

    use super::{Msg, catalog, tr_in};

    fluent_templates::static_loader! {
        static THROWAWAY = {
            locales: "./tests/locales",
            fallback_language: "qaa",
            customise: |bundle| bundle.set_use_isolating(false),
        };
    }

    /// Every placeable the catalog at `path` names, each set to a number, so
    /// a message that selects a form by one formats whole.
    fn placeables(path: &Path) -> HashMap<Cow<'static, str>, FluentValue<'static>> {
        let source = std::fs::read_to_string(path).expect("the catalog");
        source
            .split("{ $")
            .skip(1)
            .filter_map(|rest| rest.split([' ', '}']).next())
            .map(|name| (Cow::Owned(name.to_owned()), FluentValue::from(3)))
            .collect()
    }

    /// The directories under `locales/`, each a language.
    fn catalog_directories(root: &Path) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(root)
            .expect("the locales directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        found.sort();
        found
    }

    #[test]
    fn every_catalog_directory_is_a_language_holding_every_message() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
        let found = catalog_directories(&root);
        let mut shipped: Vec<String> = catalog().locales().map(ToString::to_string).collect();
        shipped.sort();
        assert_eq!(found, shipped, "a catalog directory is a language");
        assert!(
            found.contains(&String::from("en")),
            "English is the reference"
        );
        for tag in found {
            let language = tag.parse().expect("a language");
            let args = placeables(&root.join(&tag).join("ferroterm-viewer.ftl"));
            for msg in Msg::ALL {
                let words = catalog().try_lookup_with_args(&language, msg.id(), &args);
                assert!(
                    words.is_some_and(|words| !words.trim().is_empty()),
                    "{tag} holds {}",
                    msg.id()
                );
            }
        }
    }

    #[test]
    fn a_throwaway_catalog_changes_the_words_with_no_code_change() {
        let english = Locale::english();
        assert_eq!("Not found", tr_in(catalog(), &english, Msg::NotFoundTitle));
        let throwaway = Locale::parse("qaa").expect("a private-use language");
        assert_eq!(
            "Qot qound",
            tr_in(&*THROWAWAY, &throwaway, Msg::NotFoundTitle)
        );
        assert_eq!(
            "Back to the dashboard",
            tr_in(catalog(), &throwaway, Msg::NotFoundBack),
            "the shipped catalogs fall back to English"
        );
    }
}
