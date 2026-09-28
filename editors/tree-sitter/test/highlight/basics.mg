// a leading comment

font (name: "Metaglyph Sans", em: 1000)
//^ keyword
//    ^ property
//           ^ string
//                            ^ property
//                                ^ number

param stem (default: 100, range: 20..260)
//^ keyword
//    ^ constant
//          ^ property
//                   ^ number
//                        ^ property
//                               ^ number
//                                  ^ operator

let hair = stem * contrast;
//^ keyword
//  ^ variable
//              ^ operator

glyph six (advance: glyph.bbox.x1 + sidebear) {
//^ keyword
//    ^ type
//         ^ property
//                  ^ namespace
//                        ^ property

  anchor top (at: (1, 2))
//^ keyword
//       ^ variable
//            ^ property

  path p (stroke: hair, caps: "butt") {
//^ keyword
//     ^ variable
//        ^ property
//                      ^ property
//                             ^ string.special.symbol

    start (at: (0, 0))
//  ^ function.builtin

    line (to: (1, 1))
//  ^ function.builtin

    close
//  ^ function.builtin

  }

  component (glyph: six, transform: identity)
//^ keyword
//           ^ property
//                                  ^ constant.builtin

}

instance Bold (stem: 160, slant: 0deg)
//^ keyword
//       ^ variable
//             ^ property
//                                ^ type

kern (left: A, right: V, by: -20)
//^ keyword
//    ^ property

let round_trip = sqrt(2) * up.x;
//  ^ variable
//               ^ function.builtin
//                         ^ constant.builtin
//                            ^ property

let flag = true and not false;
//  ^ variable
//         ^ boolean
//              ^ keyword.operator
//                  ^ keyword.operator

