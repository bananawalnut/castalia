# AWS Free Plan to OCI Always Free migration

Trigger this runbook at day 120 after AWS account creation or when 25% of Free
Plan credits remain, whichever occurs first.

1. Produce and decrypt-test a fresh age-encrypted AWS node backup.
2. Provision an OCI Always Free `VM.Standard.A1.Flex` host with 2 OCPU and 12 GB
   RAM. Permit operator SSH and public 80/443 only.
3. Build the same revision for Linux AArch64 in a protected verified-Lean CI
   job. Do not reuse the x86-64 AWS binary and do not compile on OCI.
4. Install the AArch64 artifact with the same solo service and Caddy route
   boundary.
5. Stop the AWS node, make the final encrypted backup, transfer it, and restore
   the node key, genesis, and ledger on OCI.
6. Run local preflight on OCI before changing DNS.
7. Move the `dregg.zenith-research.ca` A record to the OCI public address.
8. Verify HTTPS, the same node identity, the existing membership cell, one
   idempotent Join retry, and a node restart.
9. Delete the AWS CloudFormation stack and confirm that its Elastic IP and EBS
   volume are gone before Free Plan expiry.

The stable hostname keeps Wallet host permission and persisted `nodeUrl`
bindings valid. The migration changes hosting, not the membership contract or
ledger identity.
