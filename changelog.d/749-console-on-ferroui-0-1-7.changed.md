- The console draws with FerroUI 0.1.7 and is ready for more languages,
  English the one that ships (#749). The components' words and the console's
  come from Fluent catalogs, the console's under
  `app/ferroterm-viewer/locales/`, so a language joins as a catalog file with
  no code change. The locale is chosen once per request, the best language of
  `Accept-Language` the console's catalog holds or else the new
  `[server] locale` (default `en`), and the page writes it as `<html lang>`.
  `scripts/checks/stray-literals.sh` fails CI on a console string outside the
  catalog beyond the per-file baseline the remaining strings hold while they
  move. The review capture waits for the page height to hold before it
  measures a screen, so the concept browser is shot with its list.
