- `_elements` keeps every top-level element the resource's definition at the
  served version makes mandatory, beside the elements asked for, so a
  `CodeSystem` listed with `_elements=url,name,title,version,status,meta`
  still carries `content` and decodes as a valid resource.
