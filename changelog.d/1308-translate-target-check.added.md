- `ConceptMap/$translate` checks every target code it returns against its
  code system on every version, as the terminology ecosystem IG 1.11.0 asks:
  a target code its code system does not hold keeps its match and adds one
  `error` parameter, and a match whose target code system the server does not
  know carries a `note` part saying the code has not been validated. A code
  system whose content is not complete cannot call a code invalid, so it
  triggers neither. The console shows both beside the server's message.
