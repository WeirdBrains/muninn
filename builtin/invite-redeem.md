# invite-redeem (built in, global)

Checks an invite link for this organization before you accept it.

input:  { "link": "muninn://join/…" }
output: { "valid": bool, "org": name, "role": role name, "expires": epoch ms,
          "next": "mnnn org join LINK" }

It reads the link only to find the invitation it belongs to; it does not use it up,
store it or log it. Accepting it (`mnnn org join`) starts the key exchange from your
own node, which is where your keys live.
