// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title ArchiLedger
 * @dev Contrato soberano para anclar Archi-Trace-Hashes (pruebas de ejecución) en la blockchain.
 * Diseñado bajo los principios de la Ley Ω₉ (Verdad Inmutable).
 */
contract ArchiLedger {
    
    struct Trace {
        string agentId;
        string domain;
        uint256 timestamp;
        address submitter;
    }

    // Mapping de Trace Hash (bytes32) a los detalles de la traza
    mapping(bytes32 => Trace) public traces;

    // Evento emitido cuando una traza es anclada exitosamente
    event TraceAnchored(
        bytes32 indexed traceHash,
        string agentId,
        string domain,
        uint256 timestamp,
        address indexed submitter
    );

    /**
     * @dev Registra un nuevo Archi-Trace-Hash de manera inmutable.
     * @param _traceHash El hash criptográfico de la ejecución (SHA-256 convertido a bytes32)
     * @param _agentId Identificador único del agente que ejecutó la tarea
     * @param _domain Dominio de la operación (ej. "finance", "audit")
     */
    function anchorTrace(
        bytes32 _traceHash,
        string calldata _agentId,
        string calldata _domain
    ) external {
        // Asegurar que el hash no ha sido registrado previamente (Prevención de replay)
        require(traces[_traceHash].timestamp == 0, "Trace already anchored");

        // Almacenar los datos de la traza
        traces[_traceHash] = Trace({
            agentId: _agentId,
            domain: _domain,
            timestamp: block.timestamp,
            submitter: msg.sender
        });

        // Emitir evento para indexación off-chain
        emit TraceAnchored(_traceHash, _agentId, _domain, block.timestamp, msg.sender);
    }

    /**
     * @dev Verifica si un hash específico existe en el ledger on-chain.
     * @param _traceHash El hash a verificar
     * @return bool True si el hash existe y es válido
     */
    function verifyTrace(bytes32 _traceHash) external view returns (bool) {
        return traces[_traceHash].timestamp != 0;
    }
}
