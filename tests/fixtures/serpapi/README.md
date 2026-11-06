# SerpApi Google Flights fixtures

No live SerpApi call was made for these: no key was available when they were
written. They are modelled on the reply shapes documented at
<https://serpapi.com/google-flights-api> (the "Typical Example", the
`departure_id: CDG,ORY` multiple-airports example and the "JSON structure
overview"), trimmed to the fields hackathon-radar reads plus a few neighbours,
with the route, dates and prices changed to a Brussels → Munich round trip.

- `bru-crl-muc.json`: a round trip from `BRU,CRL` to `MUC`, 4–7 Dec 2026, in
  EUR. `best_flights` and `other_flights` both hold options; the cheapest
  (96) leaves from CRL, and one option has no `price`.
- `no-results.json`: the reply when Google Flights finds nothing, with the
  `error` message SerpApi gives for empty results.
- `error.json`: an error reply, such as an invalid key.

If a recorded reply replaces one of these, scrub `api_key` from its
`search_parameters` and from every URL in `search_metadata`.
