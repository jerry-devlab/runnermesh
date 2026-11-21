# P03 Owner operator commands

The immutable Windows x64 package helper provides two explicit commands for the
existing Owner authorization protocol.

| Command | Private input | Output |
|---|---|---|
| owner-issue | One JSON value with scope, nonce and authority_key | Signed voucher on redirected standard output |
| owner-submit with absolute accepted ZIP, approved ZIP SHA-256 and absolute installation root | One signed voucher on redirected standard input | Transaction ID, expiry and authorization result |

Run owner-issue offline under Owner custody. Supply the exact scope and a unique
nonce through a private input stream. The key is 32 bytes encoded as a JSON byte
array. The authority key is never an argument, a token in ordinary configuration
or part of the output. Capture the voucher only in an approved private output
stream. Avoid terminals, shell transcripts, shared folders, repositories and logs
for either input or voucher output.

The scope contains service_name, image_token, execution_identity_token,
target_host_token, candidate_commit, transaction_id, allow_start, allow_stop and
allow_change_config. The issuer signs an exact actions.runner service, requires
at least one Start or Stop permission, refuses ChangeConfig and uses a
30-minute window on the issuer clock. It requires the scope commit to match
the immutable helper's build commit.

Owner-submit verifies the approved archive and the same-user pipe server's
installed Agent image against its manifest. Build provenance and voucher
submission use one connection. The SCM target kind is signed and matched to one
enrolled registry record before consumption. The Agent validates the request
against its host, commit, trusted clock and configured authority. It records
consumption in the ledger under the installation's Agent state directory
before arming the selected backend. An unreadable or malformed ledger prevents
authorization. An ambiguous submission response requires readback before any
further action.


Credential Manager provisioning, access to real key material, real issuance,
access to an installed runtime and service transactions require their own Owner
authorization. Validate the exact service and repository runner association and
capture the restore prestate before scheduling a live transaction.
