```
# SpecialCasing-17.0.0.txt
# Date: &#x2025;-07-31, 22:11:55 GMT
# © &#x2025; Unicode®, Inc.
# Unicode and the Unicode Logo are registered trademarks of Unicode, Inc. in the U.S. and other countries.
# For terms of use and license, see https://www.unicode.org/terms_of_use.html
#
# Unicode Character Database
#   For documentation, see https://www.unicode.org/reports/tr44/
#
# Special Casing
#
# This file is a supplement to the UnicodeData.txt file. The data in this file, combined with
# the simple case mappings in UnicodeData.txt, defines the full case mappings
# Lowercase_Mapping (lc), Titlecase_Mapping (tc), and Uppercase_Mapping (uc).
# For compatibility, the UnicodeData.txt file only contains simple case mappings
# for characters where they are one-to-one (and independent of context and language).
#
# For historical reasons, this file also provides additional information about the casing
# of Unicode characters for selected situations when casing is dependent on context or locale.
#
# Note that the preferred mechanism for defining tailored casing operations is
# the Unicode Common Locale Data Repository (CLDR). For more information, see the
# discussion of case mappings and case algorithms in the Unicode Standard.
#
# All code points not listed in this file that do not have simple case mappings
# in UnicodeData.txt map to themselves.
# ================================================================================
# Format
# ================================================================================
# The entries in this file are in the following machine-readable format:
#
# <code>; <lower>; <title>; <upper>; (<condition_list>;)? # <comment>
#
# <code>, <lower>, <title>, and <upper> provide the respective full case mappings
# of <code>, expressed as character values in hex. If there is more than one character,
# they are separated by spaces. Other than as used to separate elements, spaces are
# to be ignored.
#
# The <condition_list> is optional. Where present, it consists of one or more language IDs
# or casing contexts, separated by spaces. In these conditions:
# - A condition list overrides the normal behavior if all of the listed conditions are true.
# - The casing context is always the context of the characters in the original string,
#   NOT in the resulting string.
# - Case distinctions in the condition list are not significant.
# - Conditions preceded by "Not_" represent the negation of the condition.
# The condition list is not represented in the UCD as a formal property.
#
# A language ID is defined by BCP 47, with '-' and '_' treated equivalently.
#
# A casing context for a character is defined in the
# "Conformance" / "Default Case Algorithms" section of the core specification.
#
# Parsers of this file must be prepared to deal with future additions to this format:
#  * Additional contexts
#  * Additional fields
# ================================================================================
```

```
# ================================================================================
# Unconditional mappings
# The mappings in this section are not language-sensitive nor context-sensitive.
#
# Note that comments provide additional information but
# do not modify the case mapping algorithms in the core specification, chapter 3.
# ================================================================================
```

```
# The German es-zed is special--the normal mapping is to SS.
# Note: the titlecase should never occur in practice. It is equal to titlecase(uppercase(<es-zed>))
```

# &#x00DF;;&#x00DF;;&#x0053;&#x0073;;&#x0053;&#x0053;; # LATIN SMALL LETTER SHARP S

```
# Preserve canonical equivalence for I with dot. Turkic is handled below.
```

# &#x0130;;&#x0069;&#x0307;;&#x0130;;&#x0130;; # LATIN CAPITAL LETTER I WITH DOT ABOVE

```
# Ligatures
```

# &#xFB00;;&#xFB00;;&#x0046;&#x0066;;&#x0046;&#x0046;; # LATIN SMALL LIGATURE FF
# &#xFB01;;&#xFB01;;&#x0046;&#x0069;;&#x0046;&#x0049;; # LATIN SMALL LIGATURE FI
# &#xFB02;;&#xFB02;;&#x0046;&#x006C;;&#x0046;&#x004C;; # LATIN SMALL LIGATURE FL
# &#xFB03;;&#xFB03;;&#x0046;&#x0066;&#x0069;;&#x0046;&#x0046;&#x0049;; # LATIN SMALL LIGATURE FFI
# &#xFB04;;&#xFB04;;&#x0046;&#x0066;&#x006C;;&#x0046;&#x0046;&#x004C;; # LATIN SMALL LIGATURE FFL
# &#xFB05;;&#xFB05;;&#x0053;&#x0074;;&#x0053;&#x0054;; # LATIN SMALL LIGATURE LONG S T
# &#xFB06;;&#xFB06;;&#x0053;&#x0074;;&#x0053;&#x0054;; # LATIN SMALL LIGATURE ST

# &#x0587;;&#x0587;;&#x0535;&#x0582;;&#x0535;&#x0552;; # ARMENIAN SMALL LIGATURE ECH YIWN
# &#xFB13;;&#xFB13;;&#x0544;&#x0576;;&#x0544;&#x0546;; # ARMENIAN SMALL LIGATURE MEN NOW
# &#xFB14;;&#xFB14;;&#x0544;&#x0565;;&#x0544;&#x0535;; # ARMENIAN SMALL LIGATURE MEN ECH
# &#xFB15;;&#xFB15;;&#x0544;&#x056B;;&#x0544;&#x053B;; # ARMENIAN SMALL LIGATURE MEN INI
# &#xFB16;;&#xFB16;;&#x054E;&#x0576;;&#x054E;&#x0546;; # ARMENIAN SMALL LIGATURE VEW NOW
# &#xFB17;;&#xFB17;;&#x0544;&#x056D;;&#x0544;&#x053D;; # ARMENIAN SMALL LIGATURE MEN XEH

```
# No corresponding uppercase precomposed character
```

# &#x0149;;&#x0149;;&#x02BC;&#x004E;;&#x02BC;&#x004E;; # LATIN SMALL LETTER N PR&#xECED;ED BY APOSTROPHE
# &#x0390;;&#x0390;;&#x0399;&#x0308;&#x0301;;&#x0399;&#x0308;&#x0301;; # GREEK SMALL LETTER IOTA WITH DIALYTIKA AND TONOS
# &#x03B0;;&#x03B0;;&#x03A5;&#x0308;&#x0301;;&#x03A5;&#x0308;&#x0301;; # GREEK SMALL LETTER UPSILON WITH DIALYTIKA AND TONOS
# &#x01F0;;&#x01F0;;&#x004A;&#x030C;;&#x004A;&#x030C;; # LATIN SMALL LETTER J WITH CARON
# &#x1E96;;&#x1E96;;&#x0048;&#x0331;;&#x0048;&#x0331;; # LATIN SMALL LETTER H WITH LINE BELOW
# &#x1E97;;&#x1E97;;&#x0054;&#x0308;;&#x0054;&#x0308;; # LATIN SMALL LETTER T WITH DIAERESIS
# &#x1E98;;&#x1E98;;&#x0057;&#x030A;;&#x0057;&#x030A;; # LATIN SMALL LETTER W WITH RING ABOVE
# &#x1E99;;&#x1E99;;&#x0059;&#x030A;;&#x0059;&#x030A;; # LATIN SMALL LETTER Y WITH RING ABOVE
# &#x1E9A;;&#x1E9A;;&#x0041;&#x02BE;;&#x0041;&#x02BE;; # LATIN SMALL LETTER A WITH RIGHT HALF RING
# &#x1F50;;&#x1F50;;&#x03A5;&#x0313;;&#x03A5;&#x0313;; # GREEK SMALL LETTER UPSILON WITH PSILI
# &#x1F52;;&#x1F52;;&#x03A5;&#x0313;&#x0300;;&#x03A5;&#x0313;&#x0300;; # GREEK SMALL LETTER UPSILON WITH PSILI AND VARIA
# &#x1F54;;&#x1F54;;&#x03A5;&#x0313;&#x0301;;&#x03A5;&#x0313;&#x0301;; # GREEK SMALL LETTER UPSILON WITH PSILI AND OXIA
# &#x1F56;;&#x1F56;;&#x03A5;&#x0313;&#x0342;;&#x03A5;&#x0313;&#x0342;; # GREEK SMALL LETTER UPSILON WITH PSILI AND PERISPOMENI
# &#x1FB6;;&#x1FB6;;&#x0391;&#x0342;;&#x0391;&#x0342;; # GREEK SMALL LETTER ALPHA WITH PERISPOMENI
# &#x1FC6;;&#x1FC6;;&#x0397;&#x0342;;&#x0397;&#x0342;; # GREEK SMALL LETTER ETA WITH PERISPOMENI
# &#x1FD2;;&#x1FD2;;&#x0399;&#x0308;&#x0300;;&#x0399;&#x0308;&#x0300;; # GREEK SMALL LETTER IOTA WITH DIALYTIKA AND VARIA
# &#x1FD3;;&#x1FD3;;&#x0399;&#x0308;&#x0301;;&#x0399;&#x0308;&#x0301;; # GREEK SMALL LETTER IOTA WITH DIALYTIKA AND OXIA
# &#x1FD6;;&#x1FD6;;&#x0399;&#x0342;;&#x0399;&#x0342;; # GREEK SMALL LETTER IOTA WITH PERISPOMENI
# &#x1FD7;;&#x1FD7;;&#x0399;&#x0308;&#x0342;;&#x0399;&#x0308;&#x0342;; # GREEK SMALL LETTER IOTA WITH DIALYTIKA AND PERISPOMENI
# &#x1FE2;;&#x1FE2;;&#x03A5;&#x0308;&#x0300;;&#x03A5;&#x0308;&#x0300;; # GREEK SMALL LETTER UPSILON WITH DIALYTIKA AND VARIA
# &#x1FE3;;&#x1FE3;;&#x03A5;&#x0308;&#x0301;;&#x03A5;&#x0308;&#x0301;; # GREEK SMALL LETTER UPSILON WITH DIALYTIKA AND OXIA
# &#x1FE4;;&#x1FE4;;&#x03A1;&#x0313;;&#x03A1;&#x0313;; # GREEK SMALL LETTER RHO WITH PSILI
# &#x1FE6;;&#x1FE6;;&#x03A5;&#x0342;;&#x03A5;&#x0342;; # GREEK SMALL LETTER UPSILON WITH PERISPOMENI
# &#x1FE7;;&#x1FE7;;&#x03A5;&#x0308;&#x0342;;&#x03A5;&#x0308;&#x0342;; # GREEK SMALL LETTER UPSILON WITH DIALYTIKA AND PERISPOMENI
# &#x1FF6;;&#x1FF6;;&#x03A9;&#x0342;;&#x03A9;&#x0342;; # GREEK SMALL LETTER OMEGA WITH PERISPOMENI

```
# IMPORTANT-when iota-subscript (&#x0345;) is uppercased or titlecased,
#  the result will be incorrect unless the iota-subscript is moved to the end
#  of any sequence of combining marks. Otherwise, the accents will go on the capital iota.
#  This process can be achieved by first transforming the text to NFC before casing.
#  E.g. <alpha><iota_subscript><acute> is uppercased to <ALPHA><acute><IOTA>
```

```
# The following cases are already in the UnicodeData.txt file, so are only commented here.
```

```
# &#x0345;;&#x0345;;&#x0399;;&#x0399;; # COMBINING GREEK YPOGEGRAMMENI
```

```
# All letters with YPOGEGRAMMENI (iota-subscript) or PROSGEGRAMMENI (iota adscript)
# have special uppercases.
# Note: characters with PROSGEGRAMMENI are actually titlecase, not uppercase!
```

# &#x1F80;;&#x1F80;;&#x1F88;;&#x1F08;&#x0399;; # GREEK SMALL LETTER ALPHA WITH PSILI AND YPOGEGRAMMENI
# &#x1F81;;&#x1F81;;&#x1F89;;&#x1F09;&#x0399;; # GREEK SMALL LETTER ALPHA WITH DASIA AND YPOGEGRAMMENI
# &#x1F82;;&#x1F82;;&#x1F8A;;&#x1F0A;&#x0399;; # GREEK SMALL LETTER ALPHA WITH PSILI AND VARIA AND YPOGEGRAMMENI
# &#x1F83;;&#x1F83;;&#x1F8B;;&#x1F0B;&#x0399;; # GREEK SMALL LETTER ALPHA WITH DASIA AND VARIA AND YPOGEGRAMMENI
# &#x1F84;;&#x1F84;;&#x1F8C;;&#x1F0C;&#x0399;; # GREEK SMALL LETTER ALPHA WITH PSILI AND OXIA AND YPOGEGRAMMENI
# &#x1F85;;&#x1F85;;&#x1F8D;;&#x1F0D;&#x0399;; # GREEK SMALL LETTER ALPHA WITH DASIA AND OXIA AND YPOGEGRAMMENI
# &#x1F86;;&#x1F86;;&#x1F8E;;&#x1F0E;&#x0399;; # GREEK SMALL LETTER ALPHA WITH PSILI AND PERISPOMENI AND YPOGEGRAMMENI
# &#x1F87;;&#x1F87;;&#x1F8F;;&#x1F0F;&#x0399;; # GREEK SMALL LETTER ALPHA WITH DASIA AND PERISPOMENI AND YPOGEGRAMMENI
# &#x1F88;;&#x1F80;;&#x1F88;;&#x1F08;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH PSILI AND PROSGEGRAMMENI
# &#x1F89;;&#x1F81;;&#x1F89;;&#x1F09;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH DASIA AND PROSGEGRAMMENI
# &#x1F8A;;&#x1F82;;&#x1F8A;;&#x1F0A;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH PSILI AND VARIA AND PROSGEGRAMMENI
# &#x1F8B;;&#x1F83;;&#x1F8B;;&#x1F0B;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH DASIA AND VARIA AND PROSGEGRAMMENI
# &#x1F8C;;&#x1F84;;&#x1F8C;;&#x1F0C;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH PSILI AND OXIA AND PROSGEGRAMMENI
# &#x1F8D;;&#x1F85;;&#x1F8D;;&#x1F0D;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH DASIA AND OXIA AND PROSGEGRAMMENI
# &#x1F8E;;&#x1F86;;&#x1F8E;;&#x1F0E;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH PSILI AND PERISPOMENI AND PROSGEGRAMMENI
# &#x1F8F;;&#x1F87;;&#x1F8F;;&#x1F0F;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH DASIA AND PERISPOMENI AND PROSGEGRAMMENI
# &#x1F90;;&#x1F90;;&#x1F98;;&#x1F28;&#x0399;; # GREEK SMALL LETTER ETA WITH PSILI AND YPOGEGRAMMENI
# &#x1F91;;&#x1F91;;&#x1F99;;&#x1F29;&#x0399;; # GREEK SMALL LETTER ETA WITH DASIA AND YPOGEGRAMMENI
# &#x1F92;;&#x1F92;;&#x1F9A;;&#x1F2A;&#x0399;; # GREEK SMALL LETTER ETA WITH PSILI AND VARIA AND YPOGEGRAMMENI
# &#x1F93;;&#x1F93;;&#x1F9B;;&#x1F2B;&#x0399;; # GREEK SMALL LETTER ETA WITH DASIA AND VARIA AND YPOGEGRAMMENI
# &#x1F94;;&#x1F94;;&#x1F9C;;&#x1F2C;&#x0399;; # GREEK SMALL LETTER ETA WITH PSILI AND OXIA AND YPOGEGRAMMENI
# &#x1F95;;&#x1F95;;&#x1F9D;;&#x1F2D;&#x0399;; # GREEK SMALL LETTER ETA WITH DASIA AND OXIA AND YPOGEGRAMMENI
# &#x1F96;;&#x1F96;;&#x1F9E;;&#x1F2E;&#x0399;; # GREEK SMALL LETTER ETA WITH PSILI AND PERISPOMENI AND YPOGEGRAMMENI
# &#x1F97;;&#x1F97;;&#x1F9F;;&#x1F2F;&#x0399;; # GREEK SMALL LETTER ETA WITH DASIA AND PERISPOMENI AND YPOGEGRAMMENI
# &#x1F98;;&#x1F90;;&#x1F98;;&#x1F28;&#x0399;; # GREEK CAPITAL LETTER ETA WITH PSILI AND PROSGEGRAMMENI
# &#x1F99;;&#x1F91;;&#x1F99;;&#x1F29;&#x0399;; # GREEK CAPITAL LETTER ETA WITH DASIA AND PROSGEGRAMMENI
# &#x1F9A;;&#x1F92;;&#x1F9A;;&#x1F2A;&#x0399;; # GREEK CAPITAL LETTER ETA WITH PSILI AND VARIA AND PROSGEGRAMMENI
# &#x1F9B;;&#x1F93;;&#x1F9B;;&#x1F2B;&#x0399;; # GREEK CAPITAL LETTER ETA WITH DASIA AND VARIA AND PROSGEGRAMMENI
# &#x1F9C;;&#x1F94;;&#x1F9C;;&#x1F2C;&#x0399;; # GREEK CAPITAL LETTER ETA WITH PSILI AND OXIA AND PROSGEGRAMMENI
# &#x1F9D;;&#x1F95;;&#x1F9D;;&#x1F2D;&#x0399;; # GREEK CAPITAL LETTER ETA WITH DASIA AND OXIA AND PROSGEGRAMMENI
# &#x1F9E;;&#x1F96;;&#x1F9E;;&#x1F2E;&#x0399;; # GREEK CAPITAL LETTER ETA WITH PSILI AND PERISPOMENI AND PROSGEGRAMMENI
# &#x1F9F;;&#x1F97;;&#x1F9F;;&#x1F2F;&#x0399;; # GREEK CAPITAL LETTER ETA WITH DASIA AND PERISPOMENI AND PROSGEGRAMMENI
# &#x1FA0;;&#x1FA0;;&#x1FA8;;&#x1F68;&#x0399;; # GREEK SMALL LETTER OMEGA WITH PSILI AND YPOGEGRAMMENI
# &#x1FA1;;&#x1FA1;;&#x1FA9;;&#x1F69;&#x0399;; # GREEK SMALL LETTER OMEGA WITH DASIA AND YPOGEGRAMMENI
# &#x1FA2;;&#x1FA2;;&#x1FAA;;&#x1F6A;&#x0399;; # GREEK SMALL LETTER OMEGA WITH PSILI AND VARIA AND YPOGEGRAMMENI
# &#x1FA3;;&#x1FA3;;&#x1FAB;;&#x1F6B;&#x0399;; # GREEK SMALL LETTER OMEGA WITH DASIA AND VARIA AND YPOGEGRAMMENI
# &#x1FA4;;&#x1FA4;;&#x1FAC;;&#x1F6C;&#x0399;; # GREEK SMALL LETTER OMEGA WITH PSILI AND OXIA AND YPOGEGRAMMENI
# &#x1FA5;;&#x1FA5;;&#x1FAD;;&#x1F6D;&#x0399;; # GREEK SMALL LETTER OMEGA WITH DASIA AND OXIA AND YPOGEGRAMMENI
# &#x1FA6;;&#x1FA6;;&#x1FAE;;&#x1F6E;&#x0399;; # GREEK SMALL LETTER OMEGA WITH PSILI AND PERISPOMENI AND YPOGEGRAMMENI
# &#x1FA7;;&#x1FA7;;&#x1FAF;;&#x1F6F;&#x0399;; # GREEK SMALL LETTER OMEGA WITH DASIA AND PERISPOMENI AND YPOGEGRAMMENI
# &#x1FA8;;&#x1FA0;;&#x1FA8;;&#x1F68;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH PSILI AND PROSGEGRAMMENI
# &#x1FA9;;&#x1FA1;;&#x1FA9;;&#x1F69;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH DASIA AND PROSGEGRAMMENI
# &#x1FAA;;&#x1FA2;;&#x1FAA;;&#x1F6A;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH PSILI AND VARIA AND PROSGEGRAMMENI
# &#x1FAB;;&#x1FA3;;&#x1FAB;;&#x1F6B;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH DASIA AND VARIA AND PROSGEGRAMMENI
# &#x1FAC;;&#x1FA4;;&#x1FAC;;&#x1F6C;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH PSILI AND OXIA AND PROSGEGRAMMENI
# &#x1FAD;;&#x1FA5;;&#x1FAD;;&#x1F6D;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH DASIA AND OXIA AND PROSGEGRAMMENI
# &#x1FAE;;&#x1FA6;;&#x1FAE;;&#x1F6E;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH PSILI AND PERISPOMENI AND PROSGEGRAMMENI
# &#x1FAF;;&#x1FA7;;&#x1FAF;;&#x1F6F;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH DASIA AND PERISPOMENI AND PROSGEGRAMMENI
# &#x1FB3;;&#x1FB3;;&#x1FBC;;&#x0391;&#x0399;; # GREEK SMALL LETTER ALPHA WITH YPOGEGRAMMENI
# &#x1FBC;;&#x1FB3;;&#x1FBC;;&#x0391;&#x0399;; # GREEK CAPITAL LETTER ALPHA WITH PROSGEGRAMMENI
# &#x1FC3;;&#x1FC3;;&#x1FCC;;&#x0397;&#x0399;; # GREEK SMALL LETTER ETA WITH YPOGEGRAMMENI
# &#x1FCC;;&#x1FC3;;&#x1FCC;;&#x0397;&#x0399;; # GREEK CAPITAL LETTER ETA WITH PROSGEGRAMMENI
# &#x1FF3;;&#x1FF3;;&#x1FFC;;&#x03A9;&#x0399;; # GREEK SMALL LETTER OMEGA WITH YPOGEGRAMMENI
# &#x1FFC;;&#x1FF3;;&#x1FFC;;&#x03A9;&#x0399;; # GREEK CAPITAL LETTER OMEGA WITH PROSGEGRAMMENI

```
# Some characters with YPOGEGRAMMENI also have no corresponding titlecases
```

# &#x1FB2;;&#x1FB2;;&#x1FBA;&#x0345;;&#x1FBA;&#x0399;; # GREEK SMALL LETTER ALPHA WITH VARIA AND YPOGEGRAMMENI
# &#x1FB4;;&#x1FB4;;&#x0386;&#x0345;;&#x0386;&#x0399;; # GREEK SMALL LETTER ALPHA WITH OXIA AND YPOGEGRAMMENI
# &#x1FC2;;&#x1FC2;;&#x1FCA;&#x0345;;&#x1FCA;&#x0399;; # GREEK SMALL LETTER ETA WITH VARIA AND YPOGEGRAMMENI
# &#x1FC4;;&#x1FC4;;&#x0389;&#x0345;;&#x0389;&#x0399;; # GREEK SMALL LETTER ETA WITH OXIA AND YPOGEGRAMMENI
# &#x1FF2;;&#x1FF2;;&#x1FFA;&#x0345;;&#x1FFA;&#x0399;; # GREEK SMALL LETTER OMEGA WITH VARIA AND YPOGEGRAMMENI
# &#x1FF4;;&#x1FF4;;&#x038F;&#x0345;;&#x038F;&#x0399;; # GREEK SMALL LETTER OMEGA WITH OXIA AND YPOGEGRAMMENI

# &#x1FB7;;&#x1FB7;;&#x0391;&#x0342;&#x0345;;&#x0391;&#x0342;&#x0399;; # GREEK SMALL LETTER ALPHA WITH PERISPOMENI AND YPOGEGRAMMENI
# &#x1FC7;;&#x1FC7;;&#x0397;&#x0342;&#x0345;;&#x0397;&#x0342;&#x0399;; # GREEK SMALL LETTER ETA WITH PERISPOMENI AND YPOGEGRAMMENI
# &#x1FF7;;&#x1FF7;;&#x03A9;&#x0342;&#x0345;;&#x03A9;&#x0342;&#x0399;; # GREEK SMALL LETTER OMEGA WITH PERISPOMENI AND YPOGEGRAMMENI

```
# ================================================================================
# Conditional Mappings
# The remainder of this file provides conditional casing data used to produce
# full case mappings.
# ================================================================================
# Language-Insensitive Mappings
# These are characters whose full case mappings do not depend on language, but do
# depend on context (which characters come before or after). For more information
# see the header of this file and the Unicode Standard.
# ================================================================================
```

```
# Special case for final form of sigma
```

# &#x03A3;;&#x03C2;;&#x03A3;;&#x03A3;; Final_Sigma; # GREEK CAPITAL LETTER SIGMA

```
# Note: the following cases for non-final are already in the UnicodeData.txt file.
```

```
# &#x03A3;;&#x03C3;;&#x03A3;;&#x03A3;; # GREEK CAPITAL LETTER SIGMA
# &#x03C3;;&#x03C3;;&#x03A3;;&#x03A3;; # GREEK SMALL LETTER SIGMA
# &#x03C2;;&#x03C2;;&#x03A3;;&#x03A3;; # GREEK SMALL LETTER FINAL SIGMA
```

```
# Note: the following cases are not included, since they would case-fold in lowercasing
```

```
# &#x03C3;;&#x03C2;;&#x03A3;;&#x03A3;; Final_Sigma; # GREEK SMALL LETTER SIGMA
# &#x03C2;;&#x03C3;;&#x03A3;;&#x03A3;; Not_Final_Sigma; # GREEK SMALL LETTER FINAL SIGMA
```

```
# ================================================================================
# Language-Sensitive Mappings
# These are characters whose full case mappings depend on language and perhaps also
# context (which characters come before or after). For more information
# see the header of this file and the Unicode Standard.
# ================================================================================
```

```
# Lithuanian
```

```
# Lithuanian retains the dot in a lowercase i when followed by accents.
```

```
# Remove DOT ABOVE after "i" with upper or titlecase
```

# &#x0307;;&#x0307;; ; ; lt After_Soft_Dotted; # COMBINING DOT ABOVE

```
# Introduce an explicit dot above when lowercasing capital I's and J's
# whenever there are more accents above.
# (of the accents used in Lithuanian: grave, acute, tilde above, and ogonek)
```

# &#x0049;;&#x0069;&#x0307;;&#x0049;;&#x0049;; lt More_Above; # LATIN CAPITAL LETTER I
# &#x004A;;&#x006A;&#x0307;;&#x004A;;&#x004A;; lt More_Above; # LATIN CAPITAL LETTER J
# &#x012E;;&#x012F;&#x0307;;&#x012E;;&#x012E;; lt More_Above; # LATIN CAPITAL LETTER I WITH OGONEK
# &#x00CC;;&#x0069;&#x0307;&#x0300;;&#x00CC;;&#x00CC;; lt; # LATIN CAPITAL LETTER I WITH GRAVE
# &#x00CD;;&#x0069;&#x0307;&#x0301;;&#x00CD;;&#x00CD;; lt; # LATIN CAPITAL LETTER I WITH ACUTE
# &#x0128;;&#x0069;&#x0307;&#x0303;;&#x0128;;&#x0128;; lt; # LATIN CAPITAL LETTER I WITH TILDE

```
# ================================================================================
```

```
# Turkish and Azeri
```

```
# I and i-dotless; I-dot and i are case pairs in Turkish and Azeri
# The following rules handle those cases.
```

# &#x0130;;&#x0069;;&#x0130;;&#x0130;; tr; # LATIN CAPITAL LETTER I WITH DOT ABOVE
# &#x0130;;&#x0069;;&#x0130;;&#x0130;; az; # LATIN CAPITAL LETTER I WITH DOT ABOVE

```
# When lowercasing, remove dot_above in the sequence I + dot_above, which will turn into i.
# This matches the behavior of the canonically equivalent I-dot_above
```

# &#x0307;; ;&#x0307;;&#x0307;; tr After_I; # COMBINING DOT ABOVE
# &#x0307;; ;&#x0307;;&#x0307;; az After_I; # COMBINING DOT ABOVE

```
# When lowercasing, unless an I is before a dot_above, it turns into a dotless i.
```

# &#x0049;;&#x0131;;&#x0049;;&#x0049;; tr Not_Before_Dot; # LATIN CAPITAL LETTER I
# &#x0049;;&#x0131;;&#x0049;;&#x0049;; az Not_Before_Dot; # LATIN CAPITAL LETTER I

```
# When uppercasing, i turns into a dotted capital I
```

# &#x0069;;&#x0069;;&#x0130;;&#x0130;; tr; # LATIN SMALL LETTER I
# &#x0069;;&#x0069;;&#x0130;;&#x0130;; az; # LATIN SMALL LETTER I

```
# Note: the following case is already in the UnicodeData.txt file.
```

```
# &#x0131;;&#x0131;;&#x0049;;&#x0049;; tr; # LATIN SMALL LETTER DOTLESS I
```

```
# EOF
```


