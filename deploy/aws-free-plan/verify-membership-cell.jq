.found == true and
.id == $id and
.public_key == $owner and
.token_id == $token and
.state_commitment == $commitment and
.program_kind == "Cases" and
.program.kind == "Cases" and
(.program.cases | length) == 1 and
.program.cases[0].guard.kind == "Always" and
(.program.cases[0].constraints | length) == 16 and
(.program.cases[0].constraints) as $constraints |
all(range(0; 16);
  $constraints[.].kind == "Immutable" and
  $constraints[.].index == .
) and
.fields == [
  $magic, $two, $one,
  $zero, $zero, $zero, $zero, $zero, $zero, $zero, $zero, $zero,
  $one, $zero, $zero, $zero
] and
.capability_count == 0 and
.num_capabilities == 0 and
.has_delegate == false and
.has_delegation == false and
.delegate == null and
(.capabilities | length) == 0 and
(.capability_tombstones | length) == 0
