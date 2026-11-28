- An ECL refinement that mixes `AND` or the comma with `OR` without round
  brackets, such as `< 404684003 : 363698007 = * AND 116676008 = * OR
  42752001 = *`, is refused at the offending junction, as ECL 6.4
  Conjunction and Disjunction requires; before, the attribute-set junction
  bound first (#1489).
