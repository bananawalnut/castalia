/-
# EmitCertFDescriptor — emit the verified Cert-F AIR descriptor as byte-exact JSON.

`Market.CertFDescriptor.certFDescriptor` is the Lean-authored source of truth for
`circuit/descriptors/dregg-cert-f-ir2.json`. Keep this executable registered in
`scripts/emit_descriptors.py`; descriptor drift must be repaired at this source,
never hidden with an exclusion.

Run: `lake env lean --run EmitCertFDescriptor.lean`
-/
import Market.CertFDescriptor

open Dregg2.Circuit.DescriptorIR2 (emitVmJson2)
open Market.CertFDescriptor (certFDescriptor)

def main : IO Unit :=
  IO.print (emitVmJson2 certFDescriptor)
