- An ECL match search term that holds punctuation splits into words as the
  matched text does, so `{{ M mapTarget = "J45.9" }}` selects the map rows
  with `mapTarget` `J45.9` and `"J45"` those with `J45.0`, the ECL 6.10
  Member Filters examples; before, such a term matched nothing (#1491).
