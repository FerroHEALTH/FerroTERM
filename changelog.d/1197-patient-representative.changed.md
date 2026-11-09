- A `patients` issuer may set `act_represents_patient = true` on its
  `[[auth.issuer]]` table (ferrohealth-smart 0.14): a patient's token that
  carries an RFC 8693 `act` claim is then the patient's representative.
  Without the key such a token is refused `401` as no caller of the issuer,
  and on another kind of issuer the key refuses the start. A terminology
  resource sits in no patient's compartment, so the representative, like the
  patient, is refused every write with `403`. Upgrade step: set
  `act_represents_patient = true` on a `patients` issuer whose tokens name
  the person acting for the patient in `act`.
