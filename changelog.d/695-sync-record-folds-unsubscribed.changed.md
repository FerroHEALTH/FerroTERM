- A synchronisation run record counts the entries it left behind only because
  the subscription does not name them, one count per origin, in place of a
  line each, and the run summary carries the count as
  `entries_not_subscribed`; `entries_skipped` now counts only the entries left
  behind for another reason (#695).
