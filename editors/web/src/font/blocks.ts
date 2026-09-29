/** Unicode blocks offered as character sets (a subset of Blocks.txt). */
export interface Block {
    name: string;
    first: number;
    last: number;
}

export const BLOCKS: Block[] = [
    { name: "Basic Latin", first: 0x20, last: 0x7e },
    { name: "Latin-1 Supplement", first: 0xa0, last: 0xff },
    { name: "Latin Extended-A", first: 0x100, last: 0x17f },
    { name: "Latin Extended-B", first: 0x180, last: 0x24f },
    { name: "Greek and Coptic", first: 0x370, last: 0x3ff },
    { name: "Cyrillic", first: 0x400, last: 0x4ff },
    { name: "General Punctuation", first: 0x2000, last: 0x206f },
    { name: "Currency Symbols", first: 0x20a0, last: 0x20cf },
    { name: "Arrows", first: 0x2190, last: 0x21ff },
];

export function codepoints(block: Block): number[] {
    const out: number[] = [];
    for (let cp = block.first; cp <= block.last; cp++) out.push(cp);
    return out;
}
