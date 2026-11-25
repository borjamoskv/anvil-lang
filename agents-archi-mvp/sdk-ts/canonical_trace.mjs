// Deterministic JSON-domain encoding. Reject values JSON would silently discard.
export function canonicalTraceJson(value) {
    const active = new Set();
    function encode(input) {
        if (input === null || typeof input === 'boolean' || typeof input === 'string') {
            return JSON.stringify(input);
        }
        if (typeof input === 'number') {
            if (!Number.isFinite(input)) throw new TypeError('Non-finite trace number');
            return JSON.stringify(input);
        }
        if (typeof input !== 'object') throw new TypeError('Non-JSON trace value');
        if (active.has(input)) throw new TypeError('Cyclic trace');
        active.add(input);
        try {
            if (Array.isArray(input)) {
                const keys = Reflect.ownKeys(input).filter(key => key !== 'length');
                if (keys.length !== input.length) throw new TypeError('Non-JSON array properties');
                return '[' + Array.from({length: input.length}, (_, index) => {
                    const property = Object.getOwnPropertyDescriptor(input, String(index));
                    if (!property || !('value' in property)) throw new TypeError('Sparse/accessor array');
                    return encode(property.value);
                }).join(',') + ']';
            }
            const prototype = Object.getPrototypeOf(input);
            if (prototype !== null && prototype !== Object.prototype) throw new TypeError('Non-plain trace object');
            const keys = Reflect.ownKeys(input);
            if (keys.some(key => typeof key !== 'string')) throw new TypeError('Non-JSON symbol key');
            return '{' + keys.sort().map(key => {
                const property = Object.getOwnPropertyDescriptor(input, key);
                if (!property || !property.enumerable || !('value' in property)) throw new TypeError('Non-JSON property');
                return JSON.stringify(key) + ':' + encode(property.value);
            }).join(',') + '}';
        } finally {
            active.delete(input);
        }
    }
    return encode(value);
}
