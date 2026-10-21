- The console lays every screen out with FerroUI's own components and layout
  classes, takes ferrohealth-ui 0.2.2 for the indentation of a hierarchy row,
  and builds no stylesheet of its own, so the site build no longer downloads
  the Tailwind CSS standalone CLI and `docs/VERSIONS.md` drops its pin.
  `scripts/checks/viewer-tokens.sh` fails a class a screen writes that is not
  FerroUI's. The concept map's Mappings table keeps a long canonical inside
  its card (#775), Look up and Expand show one heading over their Parameters
  (#776), and the composer's value-set fieldset and the Find screen's offers
  keep FerroUI's card spacing and list markup.
