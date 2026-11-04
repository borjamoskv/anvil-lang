import Lake
open Lake DSL

package «anvil-lean» where
  leanOptions := #[
    ⟨`autoImplicit, false⟩,
    ⟨`relaxedAutoImplicit, false⟩
  ]

@[default_target]
lean_lib «Anvil» where
  roots := #[`Anvil]
