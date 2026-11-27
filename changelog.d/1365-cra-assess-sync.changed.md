- The CRA risk assessment, revision 2, covers the synchronisation service
  `ferroterm-sync`, released from v0.3.0: its purpose and environment, its
  foreseeable misuse, its assets, five risks with their controls, its status
  under each point of Annex I Part I(2), and its outbound data flows. It
  records the gaps the service has: a resource from a FHIR API has no digest
  to check, a plain `http` endpoint is not refused, the feed parser has no fuzz
  target, and a download has no size cap.
