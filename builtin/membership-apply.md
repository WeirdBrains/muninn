# membership-apply (built in, global)

How to apply to (register with) this organization.

input:  { } (or { "note": "a word for whoever approves" })
output: { "org": name, "access": members|open, "node": the org node's peer id,
          "addrs": [its addresses], "next": "mnnn org apply NODE ORG --note …" }

An application is a key exchange from your node to the org's node; an owner or admin
approves it (`mnnn org member approve`). Open orgs need no approval to browse.
