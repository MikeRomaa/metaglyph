// Mirrors the serde types in crates/mg-web/src/doc.rs.

export type Severity = "error" | "warning";

export interface DiagnosticInfo {
    /** UTF-16 offsets into the checked text. */
    from: number;
    to: number;
    severity: Severity;
    code: string;
    message: string;
}

export interface FontInfo {
    name?: string;
    version: string;
    designer?: string;
    foundry?: string;
    em?: number;
}

export interface DocState {
    version: number;
    /** False when the text has syntax errors; the canvas goes read-only. */
    parseOk: boolean;
    diagnostics: DiagnosticInfo[];
    font?: FontInfo;
    instances: string[];
    glyphCount: number;
}
