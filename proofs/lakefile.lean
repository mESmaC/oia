import Lake
open Lake DSL

package «oia-proofs» where
  leanOptions := #[
    ⟨`autoImplicit, false⟩
  ]

@[default_target]
lean_lib «OiaProofs» where
  roots := #[`OiaProofs]
