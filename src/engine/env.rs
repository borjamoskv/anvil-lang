pub enum ExecutionEnvironment {
    EVM,
    Soroban,
    Sealevel, // Firedancer SBPF
    Agnostic,
}

impl ExecutionEnvironment {
    pub fn get_lemmas(&self) -> Vec<String> {
        match self {
            ExecutionEnvironment::EVM => vec![
                "(assert (<= gas_used 30000000))".to_string(), // Block gas limit
            ],
            ExecutionEnvironment::Soroban => vec![
                "(assert (<= instruction_count 100000000))".to_string(),
            ],
            ExecutionEnvironment::Sealevel => vec![
                "(assert (<= compute_units 1400000))".to_string(),
                // Sealevel often deals with 64-bit strict sizes
                "(assert (= (bvult (bvadd pc (bv #x0000000000000008 64)) (bv #xffffffffffffffff 64)) true))".to_string(),
            ],
            ExecutionEnvironment::Agnostic => vec![],
        }
    }
}
