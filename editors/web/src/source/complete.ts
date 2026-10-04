import type {
    Completion,
    CompletionContext,
    CompletionResult,
} from "@codemirror/autocomplete";
import { autocompletion, snippet } from "@codemirror/autocomplete";
import { complete } from "../engine/client.ts";
import type { CompletionInfo } from "../engine/types.ts";

/** An identifier, as the lexer reads one. */
const WORD = /[A-Za-z_][A-Za-z0-9_]*$/;
/** Characters after which completion opens unasked: a member access and a
 * field's value. Elsewhere it opens on a word or Ctrl+Space. */
const TRIGGER = /[.:]$/;

function toCompletion(item: CompletionInfo): Completion {
    const { apply, info } = item;
    return {
        label: item.label,
        type: item.type,
        detail: item.detail,
        info: info
            ? () => {
                  const dom = document.createElement("div");
                  dom.textContent = info;
                  return dom;
              }
            : undefined,
        apply: apply && item.snippet ? snippet(apply) : apply,
    };
}

/** Completions from the engine: the LSP's context-aware completer
 * (fields, values, members, names, declaration kinds). */
async function source(
    ctx: CompletionContext,
): Promise<CompletionResult | null> {
    const word = ctx.matchBefore(WORD);
    const before = ctx.state.sliceDoc(Math.max(0, ctx.pos - 1), ctx.pos);
    if (!word && !ctx.explicit && !TRIGGER.test(before)) return null;
    const items = await complete(ctx.state.doc.toString(), ctx.pos);
    if (ctx.aborted || items.length === 0) return null;
    return {
        from: word?.from ?? ctx.pos,
        options: items.map(toCompletion),
        // Typing more of the word filters these; anything else asks again.
        validFor: /^[A-Za-z0-9_]*$/,
    };
}

export const mgCompletion = autocompletion({
    override: [source],
    icons: false,
});
