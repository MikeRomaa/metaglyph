I am designing a font editor that is built around constraints and paramters. Rather than defining splines graphically by dragging nodes around, I'd like to be able to define relationships between lines, curves, and construction lines like you would in CAD software. It would be nice to also have a text representation of the construction, kind of like METAFONT has. It would define construction geometry, font geometry, and would use variables and basic algebraic formlae to define their relationships.

The scope of this plan is to develop a spec for the entire project that includes:
- the kind of tools a user has to edit the font (e.g. construction point, construction line, constraint, etc.)
- what mathematical expressions can be used to define the font
- any algorithms or methods to convert the font definition into an actual font (ttf, otf, woff)
- the custom domain-specific language used to define the font

As part of this plan, do not:
- address actual implementation details such as language or library choice
- address user interface and styling concerns

This plan is the research needed to generate more specialized plans for implementing different parts of this system. This is not the implementation plan in and of itself.
