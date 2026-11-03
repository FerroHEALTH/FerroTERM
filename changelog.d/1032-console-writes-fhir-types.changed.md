- The console's editors and history read and write through `fhir-types`, at
  the release of the root that serves each: the code system editor, the
  concept map editor and the value set composer open the stored resource
  decoded at its release and save it written over that resource, the
  composer's preview posts its `Parameters` through the same release, and
  the history screen, the dashboard's recent writes and the answer to a
  write decode the `Bundle` and the resource at it. A stored resource or a
  history that does not decode draws the screen's error state, which names
  the release, the resource and the element path.
