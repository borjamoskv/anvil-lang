// basque-case-edge-compiler.ts

export type CaseId =
  | "ABS"
  | "ERG"
  | "DAT"
  | "GEN_POS"
  | "GEN_LOC"
  | "INES"
  | "ALL"
  | "ABL"
  | "TERM"
  | "DIR"
  | "INST"
  | "COM"
  | "BEN"
  | "CAUS"
  | "PART";

export type SemanticEdge =
  | "AGENT"
  | "THEME"
  | "UNDERGOER"
  | "RECIPIENT"
  | "BENEFICIARY"
  | "POSSESSOR"
  | "POSSESSED"
  | "LOCATION"
  | "DESTINATION"
  | "SOURCE"
  | "OF"
  | "OF_PLACE"
  | "IN"
  | "TO"
  | "FROM"
  | "UP_TO"
  | "TOWARDS"
  | "USING"
  | "WITH"
  | "FOR"
  | "BECAUSE_OF"
  | "PARTITIVE"
  | "PREDICATE_BOUND"
  | "UNKNOWN";

export type Determinism =
  | "DETERMINISTIC"
  | "FRAME_REQUIRED"
  | "HEURISTIC"
  | "UNKNOWN";

export interface MorphArg {
  surface: string;
  lemma: string;
  caseId: CaseId;
  npId?: string;
}

export interface EdgeResolution {
  caseId: CaseId;
  edge: SemanticEdge;
  determinism: Determinism;
  source: "CASE_LUT" | "PREDICATE_FRAME" | "SURFACE_GUESS" | "FALLBACK";
}

export type PredicateFrame = Partial<Record<CaseId, SemanticEdge>>;

const CASE_EDGE_LUT: Record<CaseId, EdgeResolution> = {
  ABS: {
    caseId: "ABS",
    edge: "PREDICATE_BOUND",
    determinism: "FRAME_REQUIRED",
    source: "CASE_LUT",
  },
  ERG: {
    caseId: "ERG",
    edge: "PREDICATE_BOUND",
    determinism: "FRAME_REQUIRED",
    source: "CASE_LUT",
  },
  DAT: {
    caseId: "DAT",
    edge: "PREDICATE_BOUND",
    determinism: "FRAME_REQUIRED",
    source: "CASE_LUT",
  },

  GEN_POS: {
    caseId: "GEN_POS",
    edge: "OF",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  GEN_LOC: {
    caseId: "GEN_LOC",
    edge: "OF_PLACE",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },

  INES: {
    caseId: "INES",
    edge: "IN",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  ALL: {
    caseId: "ALL",
    edge: "TO",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  ABL: {
    caseId: "ABL",
    edge: "FROM",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  TERM: {
    caseId: "TERM",
    edge: "UP_TO",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  DIR: {
    caseId: "DIR",
    edge: "TOWARDS",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },

  INST: {
    caseId: "INST",
    edge: "USING",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  COM: {
    caseId: "COM",
    edge: "WITH",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  BEN: {
    caseId: "BEN",
    edge: "FOR",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  CAUS: {
    caseId: "CAUS",
    edge: "BECAUSE_OF",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
  PART: {
    caseId: "PART",
    edge: "PARTITIVE",
    determinism: "DETERMINISTIC",
    source: "CASE_LUT",
  },
};

export const VERB_FRAMES: Record<string, PredicateFrame> = {
  // eman: give
  eman: {
    ERG: "AGENT",
    DAT: "RECIPIENT",
    ABS: "THEME",
  },

  // ikusi: see
  ikusi: {
    ERG: "AGENT",
    ABS: "THEME",
  },

  // erori: fall
  erori: {
    ABS: "UNDERGOER",
    INES: "LOCATION",
  },

  // joan: go
  joan: {
    ABS: "AGENT",
    ALL: "DESTINATION",
    ABL: "SOURCE",
  },

  // etorri: come
  etorri: {
    ABS: "AGENT",
    ABL: "SOURCE",
    ALL: "DESTINATION",
  },

  // egon: be/stay
  egon: {
    ABS: "THEME",
    INES: "LOCATION",
  },

  // eduki: have/hold
  eduki: {
    ERG: "POSSESSOR",
    ABS: "POSSESSED",
  },
};

export function resolveCaseEdge(
  arg: MorphArg,
  predicateLemma?: string
): EdgeResolution {
  const base = CASE_EDGE_LUT[arg.caseId];

  if (base.edge !== "PREDICATE_BOUND") {
    return base;
  }

  if (!predicateLemma) {
    return base;
  }

  const frame = VERB_FRAMES[predicateLemma];
  const edge = frame?.[arg.caseId];

  if (!edge) {
    return {
      caseId: arg.caseId,
      edge: "PREDICATE_BOUND",
      determinism: "FRAME_REQUIRED",
      source: "FALLBACK",
    };
  }

  return {
    caseId: arg.caseId,
    edge,
    determinism: "DETERMINISTIC",
    source: "PREDICATE_FRAME",
  };
}

export interface GraphNode {
  id: string;
  label: string;
  kind: "EVENT" | "ENTITY";
}

export interface GraphEdge {
  from: string;
  to: string;
  edge: SemanticEdge;
  caseId: CaseId;
  sourceToken: string;
  determinism: Determinism;
}

export interface CompiledClause {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export function compileClause(
  eventId: string,
  predicateLemma: string,
  args: MorphArg[]
): CompiledClause {
  const eventNode: GraphNode = {
    id: eventId,
    label: predicateLemma,
    kind: "EVENT",
  };

  const entityNodes: GraphNode[] = args.map((arg, i) => ({
    id: arg.npId ?? `entity:${arg.lemma}:${i}`,
    label: arg.lemma,
    kind: "ENTITY",
  }));

  const edges: GraphEdge[] = args.map((arg, i) => {
    const resolved = resolveCaseEdge(arg, predicateLemma);

    return {
      from: eventId,
      to: entityNodes[i].id,
      edge: resolved.edge,
      caseId: arg.caseId,
      sourceToken: arg.surface,
      determinism: resolved.determinism,
    };
  });

  return {
    nodes: [eventNode, ...entityNodes],
    edges,
  };
}

const SURFACE_SUFFIX_GUESSES: readonly [suffix: string, caseId: CaseId][] = [
  // Longest first. No tocar el orden.
  ["arengatik", "CAUS"],
  ["rengatik", "CAUS"],
  ["engatik", "CAUS"],
  ["gatik", "CAUS"],

  ["arentzat", "BEN"],
  ["rentzat", "BEN"],
  ["entzat", "BEN"],

  ["arekin", "COM"],
  ["rekin", "COM"],
  ["ekin", "COM"],

  ["arengandik", "ABL"],
  ["rengandik", "ABL"],
  ["engandik", "ABL"],
  ["gandik", "ABL"],

  ["arengana", "ALL"],
  ["rengana", "ALL"],
  ["engana", "ALL"],
  ["gana", "ALL"],

  ["etaraino", "TERM"],
  ["taraino", "TERM"],
  ["raino", "TERM"],

  ["etarantz", "DIR"],
  ["tarantz", "DIR"],
  ["rantz", "DIR"],

  ["etatik", "ABL"],
  ["tatik", "ABL"],
  ["etik", "ABL"],
  ["tik", "ABL"],

  ["etara", "ALL"],
  ["tara", "ALL"],
  ["era", "ALL"],
  ["ra", "ALL"],

  ["etako", "GEN_LOC"],
  ["tako", "GEN_LOC"],
  ["eko", "GEN_LOC"],
  ["ko", "GEN_LOC"],

  ["etan", "INES"],
  ["tan", "INES"],
  ["ean", "INES"],
  ["an", "INES"],
  ["n", "INES"],

  ["aren", "GEN_POS"],
  ["ren", "GEN_POS"],
  ["en", "GEN_POS"],

  ["ari", "DAT"],
  ["ei", "DAT"],
  ["ri", "DAT"],
  ["i", "DAT"],

  ["az", "INST"],
  ["ez", "INST"],
  ["z", "INST"],

  ["rik", "PART"],

  ["ek", "ERG"],
  ["ak", "ERG"],
  ["k", "ERG"],
];

export function guessCaseFromSurfaceToken(token: string): EdgeResolution | null {
  const normalized = token.trim().toLowerCase();

  for (const [suffix, caseId] of SURFACE_SUFFIX_GUESSES) {
    if (normalized.endsWith(suffix)) {
      return {
        caseId,
        edge: CASE_EDGE_LUT[caseId].edge,
        determinism: "HEURISTIC",
        source: "SURFACE_GUESS",
      };
    }
  }

  return null;
}

// Execution example test (only runs when executed directly)
if (typeof process !== "undefined" && process.argv[1]?.endsWith("basque-case-edge-compiler.ts")) {
  const clause = compileClause("event:eman:001", "eman", [
    {
      surface: "gizonak",
      lemma: "gizon",
      caseId: "ERG",
    },
    {
      surface: "umeari",
      lemma: "ume",
      caseId: "DAT",
    },
    {
      surface: "liburua",
      lemma: "liburu",
      caseId: "ABS",
    },
  ]);
  console.log(JSON.stringify(clause, null, 2));
}
