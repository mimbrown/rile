# Hyphenation patterns

The `.pat` files here are hyphenation patterns converted from SILE's
copies (`languages/*/hyphens*.lua`), which SILE converted from the
originals below with their headers removed. Each file is modified from
its original: reformatted, with comments removed. Each keeps its own
licence, which is not rile's. Where an original offers a choice of
licences, rile takes the one named. The headers are copied from
hyph-utf8 (https://github.com/hyphenation/tex-hyphen, commit `5684c0f51c0b`).

These languages' patterns are left out, because their only licences are
copyleft or there is none; text in them is not hyphenated:

- `cs`: GPL-2.0-or-later
- `hy`: LGPL-3.0
- `id`: GPL-2.0
- `lv`: LGPL-2.1 or GPL-2.0-or-later
- `mk`: GPL
- `ro`: no licence given
- `sr`: GPL (from hyph-sr-cyrl)

The LaTeX Project Public License and the Mozilla Public License 1.1, which
some of them are under, are in `LICENSES/`.

The `.ftl` files are SILE's own, under SILE's MIT licence (`LICENSE`).

## `af`, from `hyph-af.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Afrikaans
copyright: Copyright (C) 2013 Tilla Fick and Chris Swanepoel
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1.0 (September 2013)
authors:
    -
        name: Tilla Fick
        contact: fickm (at) unisa.ac.za
    -
        name: Chris Swanepoel
        contact: swanecj (at) unisa.ac.za
    -
        email: hyphen (at) rekenaar (dot) net
licence:
    name: MIT
    text: |-
        Permission is hereby granted, free of charge, to any person obtaining a copy
        of this software and associated documentation files (the "Software"), to deal
        in the Software without restriction, including without limitation the rights
        to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
        copies of the Software, and to permit persons to whom the Software is
        furnished to do so, subject to the following conditions:

        The above copyright notice and this permission notice shall be included in
        all copies or substantial portions of the Software.

        THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
        IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
        FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
        AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
        LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
        OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
        SOFTWARE.
```

## `as`, from `hyph-as.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Assamese
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `be`, from `hyph-be.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Belarusian
copyright: Copyright (C) 2016 Maksim Salau
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/hyphenation/hyph-be
version: 1.01 (October 2016)
authors:
  -
    name: Maksim Salau
    contact: maksim.salau (at) gmail.com
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
    text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
```

## `bg`, from `hyph-bg.tex`

Taken under: BSD-style (see the notice).

```text
copyright: Copyright (C) 2000, 2004, 2017 Anton Zinoviev
title: Hyphenation patterns for Bulgarian
version: 21 October 2017
notice: >
    This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
    The Bulgarian patterns used to come from another file,
    by Georgi Boshnakov, named bghyphen.tex
authors:
    -
        name: Anton Zinoviev
        contact: anton:lml.bas.bg
licence:
    text: > # Seems equivalent to 3-clause BSD
        This software may be used, modified, copied, distributed, and sold,
        both in source and binary form provided that the above copyright
        notice and these terms are retained. The name of the author may not
        be used to endorse or promote products derived from this software
        without prior permission.  THIS SOFTWARE IS PROVIDES "AS IS" AND
        ANY EXPRESS OR IMPLIED WARRANTIES ARE DISCLAIMED.  IN NO EVENT
        SHALL THE AUTHOR BE LIABLE FOR ANY DAMAGES ARISING IN ANY WAY OUT
        OF THE USE OF THIS SOFTWARE.
```

## `bn`, from `hyph-bn.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Bengali
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `ca`, from `hyph-ca.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Catalan
copyright: Copyright (C) December 1991-January 1995, July 2003 Gonçal Badenes
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1.11 15 July 2003 15:08:12 CET
authors:
  -
    name: Gonçal Badenes
    contact: g.badenes (at) ieee.org
  -
    name: Francina Turon
licence:
    name: LPPL
    version: 1
    or_later: true
    url: https://latex-project.org/lppl/
```

## `cop`, from `hyph-cop.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Coptic
copyright: Copyright (C) 2004—2010 Claudio Beccari
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
authors:
  -
    name: Claudio Beccari
    contact: claudio.beccari (at) gmail.com
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
    text: >
          Permission is hereby granted, free of charge, to any person
          obtaining a copy of this software and associated documentation
          files (the “Software”), to deal in the Software without
          restriction, including without limitation the rights to use,
          copy, modify, merge, publish, distribute, sublicense, and/or
          sell copies of the Software, and to permit persons to whom the
          Software is furnished to do so, subject to the following
          conditions:

          The above copyright notice and this permission notice shall be
          included in all copies or substantial portions of the Software.

          THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
          EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
          OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
          NONINFRINGEMENT.  IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
          HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
          WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
          FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
          OTHER DEALINGS IN THE SOFTWARE.
```

## `cu`, from `hyph-cu.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Church Slavonic
copyright: Copyright (C) 2016 Aleksandr Andreev, Mike Kroutikov
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1.1
authors:
  -
    name: Aleksandr Andreev
    contact: aleksandr.andreev (at) gmail.com
  -
    name: Mike Kroutikov
    contact: pgmmpk (at) gmail.com
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
    text: >
        Permission is hereby granted, free of charge, to any person obtaining
        a copy of this software and associated documentation files (the
        "Software"), to deal in the Software without restriction, including
        without limitation the rights to use, copy, modify, merge, publish,
        distribute, sublicense, and/or sell copies of the Software, and to
        permit persons to whom the Software is furnished to do so, subject to
        the following conditions:

        The above copyright notice and this permission notice shall be
        included in all copies or substantial portions of the Software.

        THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
        EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
        MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
        NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS
        BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
        ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
        CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
        SOFTWARE.
```

## `cy`, from `hyph-cy.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Welsh
copyright: Copyright (C) 1996 Yannis Haralambous
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1996
authors:
  -
    name: Yannis Haralambous
    contact: yannis.haralambous (at) imt-atlantique.fr
licence:
    - This file is available under any of these licences:
    -
        name: LPPL
        status: maintained
        maintainer: Yannis Haralambous
        version: 1.0
        or_later: true
        url: http://www.latex-project.org/lppl/lppl-1-0.html
    -
        name: MIT
        url: https://opensource.org/licences/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
```

## `da`, from `hyph-da.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Danish
copyright: Copyright (C) 1994 Frank Jensen
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2011-01-11
authors:
  -
    name: Frank Jensen
    contact: frank.jensen (at) hugin.com
licence:
    - This file is available under any of these licences:
    -
        name: LPPL
        version: 1.3
        or_later: true
        url: http://www.latex-project.org/lppl/lppl-1-3.html
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
```

## `de`

Taken under: LPPL.

```text
German Hyphenation Patterns (Reformed Orthography, 2006) `dehyphn-x' 2014-05-21 (WL)

From dehyph-exptl, the experimental German hyphenation patterns by Werner
Lemberg and others (https://ctan.org/pkg/dehyph-exptl), released under the
LaTeX Project Public License (https://www.latex-project.org/lppl/); its data
is MIT licensed in later releases.
```

## `el`, from `hyph-grc.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Ancient Greek
copyright: Copyright (C) 2008-2016 Dimitrios Filippou
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2016-05-12
authors:
  -
    name: Dimitrios Filippou
    contact: dimitrios.ap.filippou (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: LPPL
        url: http://www.latex-project.org/lppl/
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
```

## `el-monoton`, from `hyph-el-monoton.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Modern Greek, monotonic
copyright: Copyright (C) 2008-2011 Dimitrios Filippou
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 5.0
authors:
  -
    name: Dimitrios Filippou
    contact: dimitrios.ap.filippou (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: LPPL
        url: https://latex-project.org/lppl/
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
```

## `el-polyton`, from `hyph-el-polyton.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Modern Greek, polytonic
copyright: Copyright (C) 2008-2011 Dimitrios Filippou
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 5.0
authors:
  -
    name: Dimitrios Filippou
    contact: dimitrios.ap.filippou (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: LPPL
        url: https://latex-project.org/lppl/
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
```

## `en`

Taken under: Knuth's hyphen.tex terms.

```text
The Plain TeX hyphenation tables, hyphen.tex, by Donald E. Knuth and
Frank M. Liang.

The Plain TeX hyphenation tables [NOT TO BE CHANGED IN ANY WAY!]
Unlimited copying and redistribution of this file are permitted as long
as this file is not modified. Modifications are permitted, but only if
the resulting file is not named hyphen.tex.
```

## `eo`

Taken under: LPPL.

```text
Original file by Fredrick R. Brennan (copypaste@kittens.ph) in 2022, for SILE.
The patterns are from Sergio Pokrovskij's vortdivido project
(https://github.com/pok49/vortdivido, eohyph.m4), usable under the LaTeX
Project Public License (https://www.latex-project.org/lppl/).
```

## `es`, from `hyph-es.tex`

Taken under: MIT.

```text
copyright: Copyright (C) 1993, 1997 Javier Bezos, 2001-2019 Javier Bezos, CervanTeX
title: Hyphenation patterns for Spanish
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 5.0 2019-09-24
authors:
  -
    name: Javier Bezos
licence:
    name: MIT/X11
    url: https://opensource.org/licenses/MIT
    text: >
        Permission is hereby granted, free of charge, to any person obtaining a copy
        of this software and associated documentation files (the "Software"), to deal
        in the Software without restriction, including without limitation the rights
        to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
        copies of the Software, and to permit persons to whom the Software is
        furnished to do so, subject to the following conditions:

        The above copyright notice and this permission notice shall be included in
        all copies or substantial portions of the Software.

        THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
        IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
        FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
        AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
        LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
        OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
        SOFTWARE.
```

## `et`, from `hyph-et.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Estonian
copyright: Copyright (C) 2004-2015 Enn Saar
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
authors:
  -
    name: Enn Saar
    contact: saar (at) aai.ee
licence:
    - This file is dual-licensed under both these licences, at your discretion:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT.  IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LPPL
        version: 1.3
        or_later: true
        status: author-maintained
        url: http://www.latex-project.org/lppl.txt
```

## `eu`, from `hyph-eu.tex`

Taken under: Unicode-style data licence (see the notice).

```text
copyright: Copyright (C) 1997, 2008 Juan M. Aguirregabiria
title: Hyphenation patterns for Basque
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information on the package,
    and http://tp.lc.ehu.es/jma/basque.html for details on Basque hyphenation.
version: June 2008
authors:
  -
    name: Juan M. Aguirregabiria
    contact: juanmari.aguirregabiria (at) ehu.es
licence:
    text: >
        Permission is hereby granted, free of charge, to any person obtaining
        a copy of this file and any associated documentation
        (the "Data Files") to deal in the Data Files
        without restriction, including without limitation the rights to use,
        copy, modify, merge, publish, distribute, and/or sell copies of
        the Data Files, and to permit persons to whom the Data Files
        are furnished to do so, provided that
        (a) this copyright and permission notice appear with all copies
        of the Data Files,
        (b) this copyright and permission notice appear in associated
        documentation, and
        (c) there is clear notice in each modified Data File
        as well as in the documentation associated with the Data File(s)
        that the data has been modified.

        THE DATA FILES ARE PROVIDED "AS IS", WITHOUT WARRANTY OF
        ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
        WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
        NONINFRINGEMENT OF THIRD PARTY RIGHTS.
        IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS
        NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL
        DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE,
        DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
        TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
        PERFORMANCE OF THE DATA FILES.

        Except as contained in this notice, the name of a copyright holder
        shall not be used in advertising or otherwise to promote the sale,
        use or other dealings in these Data Files without prior
        written authorization of the copyright holder.
```

## `fi`, from `hyph-fi.tex`

Taken under: may be freely distributed.

```text
title: Hyphenation patterns for Finnish
copyright: Copyright (C) 1986, 1988, 1989 Kauko Saarinen
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2.2
authors:
  -
    name: Kauko Saarinen
    affiliation: Computing Centre, University of Jyväskylä, Finland
licence:
    - text: Patterns may be freely distributed
```

## `fr`, from `hyph-fr.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for French
copyright: Copyright (C) 1994-2002 Daniel Flipo, Bernard Gaulle, 2016 Arthur Reutenauer
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: V2.13 2016/05/12
authors:
    -
        name: Daniel Flipo
    -
        name: Bernard Gaulle
        note: deceased
    -
        name: Arthur Reutenauer
        contact: arthur (at) reutenauer.eu
    -
        email: cesure-l (at) gutenberg (dot} eu (dot) org
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
    text: >
        Permission is hereby granted, free of charge, to any person obtaining
        a copy of this software and associated documentation files (the
        "Software"), to deal in the Software without restriction, including
        without limitation the rights to use, copy, modify, merge, publish,
        distribute, sublicense, and/or sell copies of the Software, and to
        permit persons to whom the Software is furnished to do so, subject to
        the following conditions:

        The above copyright notice and this permission notice shall be
        included in all copies or substantial portions of the Software.

        THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
        EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
        MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
        NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS
        BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
        ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
        CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
        SOFTWARE.
```

## `fur`, from `hyph-fur.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Friulan
copyright: Copyright (C) 2012 Claudio Beccari
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1.1 2012/04/10
authors:
  -
    name: Claudio Beccari
    contact: claudio.beccari (at) gmail.com
licence:
    - This file is available under any of the following licences, at your option:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LPPL
        version: 1.3
        or_later: true
        url: http://www.latex-project.org/lppl.txt
        status: maintained
        maintainer: Claudio Beccari
```

## `ga`, from `hyph-ga.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Irish
copyright: Copyright (C) 2004-2015 Kevin P. Scannell
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information on the package,
    and http://borel.slu.edu/fleiscin/index.html for details on Irish hyphenation.
version: 1.0 (2004-01-23:15:46:12 CST)
authors:
  -
    name: Kevin P. Scannell
    contact: scanell (at) slu.edu
licence:
    - This file available under any of these licences:
    -
        name: GPL
        version: 2
        or_later: true
        url: http://www.gnu.org/licenses/gpl-2.0.html
        text: >
            This program is free software; you can redistribute it and/or
            modify it under the terms of the GNU General Public License
            as published by the Free Software Foundation; either version 2
            of the License, or (at your option) any later version.
           
            This program is distributed in the hope that it will be useful,
            but WITHOUT ANY WARRANTY; without even the implied warranty of
            MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
            GNU General Public License for more details.
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT.  IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
```

## `gl`, from `hyph-gl.tex`

Taken under: Unlicense.

```text
title: Hyphenation patterns for Galician
copyright: Copyright (C) 2006, 2007, 2008, 2010 Javier A. Múgica
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2.4 2010/04/23
authors:
  -
    name: Javier A. Múgica
    contact: javier (at) digi21.eu
licence:
    name: Unlicence
    url: https://unlicense.org/
```

## `gu`, from `hyph-gu.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Gujarati
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `hi`, from `hyph-hi.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Hindi
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `hr`, from `hyph-hr.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Croatian
copyright: Copyright (C) 1994, 1996, 2011, 2015 Igor Marinović
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
authors:
  -
    name: Igor Marinović
    contact: marinowski (at) gmail.com
licence:
    - This file is available under any of these licences:
    -
        name: LPPL
        version: 1
        or_later: true
        url: http://www.latex-project.org/lppl/lppl-1-0.html
    -
        text: >
            Permission is hereby granted, free of charge, to any person obtaining
            a copy of this file and any associated documentation
            (the "Data Files") to deal in the Data Files
            without restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, and/or sell copies of
            the Data Files, and to permit persons to whom the Data Files
            are furnished to do so, provided that
            (a) this copyright and permission notice appear with all copies 
            of the Data Files,
            (b) this copyright and permission notice appear in associated 
            documentation, and
            (c) there is clear notice in each modified Data File
            as well as in the documentation associated with the Data File(s)
            that the data has been modified.
            
            THE DATA FILES ARE PROVIDED "AS IS", WITHOUT WARRANTY OF
            ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
            WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT OF THIRD PARTY RIGHTS.
            IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS
            NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL
            DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE,
            DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER
            TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
            PERFORMANCE OF THE DATA FILES.
            
            Except as contained in this notice, the name of a copyright holder
            shall not be used in advertising or otherwise to promote the sale,
            use or other dealings in these Data Files without prior
            written authorization of the copyright holder.
```

## `hsb`, from `hyph-hsb.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Upper Sorbian
copyright: Copyright (C) 1997 Eduard Werner
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1997-03-22
authors:
  -
    name: Eduard Werner
    contact: edi.werner (at) gmx.de
licence:
    name: LPPL
    version: 1.3
    or_later: true
    url: http://www.latex-project.org/lppl.txt
```

## `hu`, from `hyph-hu.tex`

Taken under: MPL-1.1.

```text
title: Hyphenation patterns for Hungarian
copyright: Copyright (C) 2003 Bence Nagy
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2011-08-15
authors:
  -
    name: Bence Nagy
    contact: bence.nagy (at) gmail.com
licence:
    - This file is available under any of these licences, at your option:
    -
        name: MPL
        version: 1.1
        initial_developer: Bence Nagy
        contributors:
            - Bence Nagy
        url: https://www.mozilla.org/en-US/MPL/1.1/
    -
        name: GPL
        version: 2.0
        url: http://www.gnu.org/licenses/old-licenses/gpl-2.0.html
    -
        name: LGPL
        version: 2.1
        url: http://www.gnu.org/licenses/old-licenses/lgpl-2.1.html
```

## `ia`, from `hyph-ia.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Interlingua
copyright: Copyright (C) 1989-2005 Peter Kleiweg
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 0.2c 2005-06-28
authors:
  -
    name: Peter Kleiweg
    contact: p.c.j.kleiweg (at) rug.nl
licence:
    name: LPPL
    version: 1.3
    status: maintained
    maintainer: Peter Kleiweg
    url: https://latex-project.org/lppl/
```

## `is`, from `hyph-is.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Icelandic
copyright: Copyright (C) 1988, 2004 Jörgen Pind
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
authors:
  -
    name: Jörgen Pind
    affiliation: Institute of Lexicography, Univ. of Iceland
    contact: jorgen (at) lexis.hi.is
licence:
    name: LPPL
    version: 1.2
    or_later: true
    url: http://www.latex-project.org/lppl.txt
```

## `it`, from `hyph-it.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Italian
copyright: Copyright (C) 2008-2011 Claudio Beccari
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 4.9 2014/04/22
authors:
  -
    name: Claudio Beccari
    contact: claudio.beccari (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: LPPL
        version: 1.3
        or_later: true
        url: http://www.latex-project.org/lppl.txt
        status: maintained
        maintainer: Claudio Beccari, e-mail claudio dot beccari at gmail dot com
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
```

## `ka`, from `hyph-ka.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Georgian
copyright: Copyright (C) 2013 Levan Shoshiashvili
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: http://tex.tsu.ge
version: 0.3 (April 2013)
authors:
  -
    name: Levan Shoshiashvili
    contact: shoshia (at) hotmail.com
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
```

## `kk`, from `hyph-kk.tex`

Taken under: public domain.

```text
title: Hyphenation patterns for Kazakh (hyph-kk.tex)
copyright: Copyright (C) 2010 Mojca Miklavec, 2025 Robert Ermers, 2026 Arthur Rosendahl
notice: This file is part of the hyph-utf8 package.
    See https://www.hyphenation.org/tex for more information
authors:
  - name: Mojca Miklavec
  - name: Robert Ermers
  - name: Arthur Rosendahl
licence:
    name: Public domain
version: 0.1 Date 7 june 2025
```

## `kmr`, from `hyph-kmr.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Kurmanji (Northern Kurdish)
    – as spoken in Turkey and by the Kurdish diaspora in Europe
copyright: Copyright (C) 2009 Jörg Knappen, Medeni Shemdê
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1.0
authors:
  -
    name: Jörg Knappen
    contact: jknappen (at) web.de
  -
    name: Medeni Shemdê
licence:
    name: LPPL
    version: 1.3
    status: maintained
    maintainer: Jörg Knappen
    url: https://latex-project.org/lppl/lppl-1-3.html
```

## `kn`, from `hyph-kn.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Kannada
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `la`, from `hyph-la.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for modern and medieval Latin
copyright: Copyright (C) 1999-2016 Claudio Beccari
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 3.201 2016-08-28
authors:
  -
    name: Claudio Beccari
    contact: claudio.beccari (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the “Software”), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LPPL
        version: 1
        or_later: true
        url: https://latex-project.org/lppl/
```

## `lt`, from `hyph-lt.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Lithuanian
copyright: Copyright (C) 1992 Sigitas Tolušis, Yannis Haralambous
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: http://www.vtex.lt/tex/littex/index.html
authors:
  -
    name: Vytas Statulevičius
    contact: vytas (at) vtex.lt
  -
    name: Sigitas Tolušis
    contact: sigitas (at) vtex.lt
  -
    name: Yannis Haralambous
    contact: yannis.haralambous (at) telecom-bretagne.eu
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
    text: >
        Permission is hereby granted, free of charge, to any person
        obtaining a copy of this software and associated documentation
        files (the "Software"), to deal in the Software without
        restriction, including without limitation the rights to use,
        copy, modify, merge, publish, distribute, sublicense, and/or sell
        copies of the Software, and to permit persons to whom the
        Software is furnished to do so, subject to the following
        conditions:

        The above copyright notice and this permission notice shall be
        included in all copies or substantial portions of the Software.

        THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
        EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
        OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
        NONINFRINGEMENT.  IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
        HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
        WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
        FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
        OTHER DEALINGS IN THE SOFTWARE.
```

## `ml`, from `hyph-ml.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Malayalam
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `mr`, from `hyph-mr.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Marathi
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences, at your option:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `nb`

The patterns of `no`.

## `nl`, from `hyph-nl.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Dutch
copyright: Copyright (C) 1996 Piet Tutelaers
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1.1 (November 1996)
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
authors:
    -
        name: Piet Tutelaers
        contact: P.T.H.Tutelaers (at) tue.nl
```

## `nn`

The patterns of `no`.

## `no`, from `hyph-no.tex`

Taken under: all-permissive (see the notice).

```text
title: Hyphenation patterns for Norwegian
copyright: Copyright (C) 2004, 2005 Rune Kleveland, Ole Michael Selberg
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2012-05-18
    Patterns of January 25, 2005, with later additions and changes
authors:
    - name: Rune Kleveland
    - name: Ole Michael Selberg
licence:
    - text: >
        Copying and distribution of this file, with or without modification,
        is permitted in any medium without royalty provided the copyright
        notice and this notice are preserved.
```

## `oc`, from `hyph-oc.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Occitan
copyright: Copyright (C) 2016 Claudio Beccari
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 0.1 2016/02/04
authors:
  -
    name: Claudio Beccari
    contact: claudio.beccari (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the “Software”), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LPPL
        version: 1
        or_later: true
        url: https://latex-project.org/lppl/
```

## `oc-aranes`

The patterns of `oc`.

## `or`, from `hyph-or.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Oriya
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the “Software”), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `pa`, from `hyph-pa.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Panjabi
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the “Software”), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `pi`, from `hyph-pi.tex`

Taken under: MIT.

```text
copyright: Copyright (C) 2018 Wie-Ming Cittānurakkho Bhikkhu
title: Hyphenation patterns for Pāli
version: 2018-06-29
notice: >
    This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
authors:
     -
         name: Wie-Ming Cittānurakkho Bhikkhu
         contact: wiemingang:gmail.com
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
    text: >
        Permission is hereby granted, free of charge, to any person
        obtaining a copy of this software and associated documentation
        files (the “Software”), to deal in the Software without
        restriction, including without limitation the rights to use,
        copy, modify, merge, publish, distribute, sublicense, and/or sell
        copies of the Software, and to permit persons to whom the
        Software is furnished to do so, subject to the following
        conditions:

        The above copyright notice and this permission notice shall be
        included in all copies or substantial portions of the Software.

        THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
        EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
        OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
        NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
        HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
        WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
        FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
        OTHER DEALINGS IN THE SOFTWARE.
source: Generated manually
```

## `pl`, from `hyph-pl.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Polish
copyright: Copyright (C) 1987-1995 Hanna Kołodziejska, Bogusław Jackowski, Marek Ryćko
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 3.0a, Wednesday, May 17th, 1995
authors:
  -
    name: Hanna Kołodziejska
  -
    name: Bogusław Jackowski
    contact: b_jackowski (at) gust.org.pl
  -
    name: Marek Ryćko
    contact: marek (at) do.com.pl
licence:
    - This file is made available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the “Software”), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    - text: >
        This macro file belongs to the public domain
        under the conditions specified by the author of TeX:

           “Macro files like PLAIN.TEX should not be changed in any way,
            except with respect to preloaded fonts,
            unless the changes are authorized by the authors of the macros.”

                                                  Donald E. Knuth
```

## `pt`, from `hyph-pt.tex`

Taken under: BSD-3-Clause.

```text
title: Hyphenation patterns for Portuguese
copyright: Copyright (C) 1987, 1994, 1996, 2015 Pedro J. de Rezende, 1996, 2015 J. Joao Dias Almeida, 2024 Leonardo Araujo and Aline Benevides
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1.4 2024-07-13
authors:
  -
    name: Pedro J. de Rezende
    contact: rezende (at) ic.unicamp.br
  -
    name: J. Joao Dias Almeida
    contact: jj (at) di.uminho.pt
  -
    name: Leonardo Araujo
    contact: leolca (at) gmail.com
  -
    name: Aline Benevides
    contact: benevides.aline12 (at) gmail.com
licence:
    name: BSD 3-clause licence
    url: https://opensource.org/licenses/BSD-3-Clause
    text: >
        Redistribution and use in source and binary forms, with or without
        modification, are permitted provided that the following conditions
        are met:
        * Redistributions of source code must retain the above copyright
          notice, this list of conditions and the following disclaimer.
        * Redistributions in binary form must reproduce the above copyright
          notice, this list of conditions and the following disclaimer in the
          documentation and/or other materials provided with the
          distribution.
        * Neither the name of the University of Campinas, of the University
          of Minho nor the names of its contributors may be used to endorse
          or promote products derived from this software without specific
          prior written permission.

        THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
        "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
        LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
        A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL PEDRO J. DE
        REZENDE OR J.JOAO DIAS ALMEIDA BE LIABLE FOR ANY DIRECT, INDIRECT,
        INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
        BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
        OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
        AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
        LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY
        WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
        POSSIBILITY OF SUCH DAMAGE.
```

## `rm`, from `hyph-rm.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Romansh
copyright: Copyright (C) 2012 Claudio Beccari, 2024 far.ch
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2.0 2024-10-29
authors:
  - name: Claudio Beccari
    contact: claudio.beccari (at) gmail.com
  -
    name: Martin Cantieni, far.ch
    contact: da@far.ch
licence:
    - This file may be distributed under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LPPL
        version: 1.3
        or_later: true
        status: maintained
        maintainer: far.ch, da@far.ch
```

## `ru`, from `hyph-ru.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Russian
copyright: Copyright (C) 1999-2003 Alexander I. Lebedev
notice:
    This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2003/03/10
authors:
    -
        name: Alexander I. Lebedev
        contact: swan (at) scon155.phys.msu.su
licence:
    name: LPPL
    version: 1.2
    or_later: true
    url: https://latex-project.org/lppl/
```

## `sa`, from `hyph-sa.tex`

Taken under: free to use, copy, modify and distribute.

```text
title: Hyphenation patterns for Sanskrit and Prakrit, Latin transliteration,
    and Devanāgari, Bengali, Gujarati, Kannada, Malayalam, and Telugu scripts
copyright: Copyright (C) 2006-2011 Yves Codet
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 0.6
authors:
  -
    name: Yves Codet
    contact: ycodet (at) club-internet.fr
licence:
    text: You may freely use, copy, modify and/or distribute this file.
```

## `sk`, from `hyph-sk.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Slovak
copyright: Copyright (C) 1992 Jana Chlebíková
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2.0 1992-04-24
authors:
  -
    name: Jana Chlebíková
    affiliation: Department of Informatics Education, Comenius University, 842 15 Bratislava, Slovakia 
    contact: chlebikj (at) dcs.fmph.uniba.sk
licence:
    name: MIT
    url: http://www.opensource.org/licenses/MIT
```

## `sl`, from `hyph-sl.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Slovenian
copyright: Copyright (C) 1990 Matjaž Vrečko
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2.3
authors:
  -
    name: Matjaž Vrečko
    affiliation: TeXCeX
    contact: matjaz (at) mg-soft.si
licence:
    - This file is available under any of these licences:
    -
        name: LPPL
        version: 1
        or_later: true
        url: http://www.latex-project.org/lppl/lppl-1-0.html
    -
        name: MIT
        url: https://opensource.org/licences/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
```

## `sq`, from `hyph-sq.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Albanian
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.

version: 2025-01-10

authors:
    -
        name: Claudio Beccari
        contact: claudio.beccari@gmail.com
    -
        name: Sabina Koliqi
        contact: sabina.kolici@enaip.piemonte.it
    -
        name: Arthur Rosendahl
        contact: arthur@rosendahl.io

copyright: >
    Copyright (C) 2020 Claudio Beccari, Sabina Koliqi
              (C) 2025 Arthur Rosendahl

licence:
    name: MIT
    url:  http://opensource.org/licenses/mit-license.php
    text: >
          Permission is hereby granted, free of charge, to any person
          obtaining a copy of this software and associated documentation
          files (the “Software”), to deal in the Software without
          restriction, including without limitation the rights to use,
          copy, modify, merge, publish, distribute, sublicense, and/or
          sell copies of the Software, and to permit persons to whom the
          Software is furnished to do so, subject to the following
          conditions:

          The above copyright notice and this permission notice shall be
          included in all copies or substantial portions of the Software.

          THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND,
          EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
          OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
          NONINFRINGEMENT.  IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
          HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
          WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
          FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
          OTHER DEALINGS IN THE SOFTWARE.
```

## `sv`, from `hyph-sv.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Swedish
copyright: Copyright (C) 1994 Jan Michael Rynning
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 1994-03-03
authors:
  -
    name: Jan Michael Rynning
    contact: jmr (at) incolumitas.se
licence:
    name: LPPL
    version: 1.2
    or_later: true
```

## `ta`, from `hyph-ta.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Tamil
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `te`, from `hyph-te.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Telugu
copyright: Copyright (C) 2016 Santhosh Thottingal
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
source: https://github.com/santhoshtr/hyphenation/
version: 0.9.0 2016-01-16
authors:
  -
    name: Santhosh Thottingal
    contact: santhosh.thottingal (at) gmail.com
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
        text: >
            Permission is hereby granted, free of charge, to any person
            obtaining a copy of this software and associated documentation
            files (the "Software"), to deal in the Software without
            restriction, including without limitation the rights to use,
            copy, modify, merge, publish, distribute, sublicense, and/or sell
            copies of the Software, and to permit persons to whom the
            Software is furnished to do so, subject to the following
            conditions:

            The above copyright notice and this permission notice shall be
            included in all copies or substantial portions of the Software.

            THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
            EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
            OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
            NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
            HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
            WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
            FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
            OTHER DEALINGS IN THE SOFTWARE.
    -
        name: LGPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/lgpl.html
    -
        name: GPL
        version: 3
        or_later: true
        url: http://www.gnu.org/licenses/gpl.html
```

## `th`, from `hyph-th.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Thai
copyright: Copyright (C) 2012-2013 Theppitak Karoonboonyanan
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
authors:
  -
    name: Theppitak Karoonboonyanan
    contact: theppitak (at) gmail.com
licence:
    name: LPPL
    version: 1.3
    or_later: true
    status: maintained
    maintainer: Theppitak Karoonboonyanan
    url: https://latex-project.org/lppl/
```

## `tk`, from `hyph-tk.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Turkmen
copyright: Copyright (C) 2010-2015 Nazar Annagurban
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 0.1 16 March 2010
authors:
  -
    name: Nazar Annagurban
    contact: nazartm (at) gmail.com
licence:
    name: MIT
    url: https://opensource.org/licenses/MIT
    text: >
        Permission is hereby granted, free of charge, to any person obtaining a copy
        of this software and associated documentation files (the "Software"), to deal
        in the Software without restriction, including without limitation the rights
        to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
        copies of the Software, and to permit persons to whom the Software is
        furnished to do so, subject to the following conditions:
        
        The above copyright notice and this permission notice shall be included in
        all copies or substantial portions of the Software.
        
        THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
        IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
        FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL THE
        AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
        LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
        OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
        THE SOFTWARE.
```

## `tr`, from `hyph-tr.tex`

Taken under: LPPL.

```text
title: Hyphenation patterns for Turkish
copyright: Copyright (C) 1987 Pierre A. MacKay, 2008, 2011 TUG
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
authors:
    -
        name: Pierre A. MacKay
    -
        name: H. Turgut Uyar
        contact: uyar (at) itu.edu.tr
    -
        name: S. Ekin Kocabas
        contact: kocabas (at) stanford.edu
    -
        name: Mojca Miklavec
licence:
    name: LPPL
    version: 1
    or_later: true
    url: https://latex-project.org/lppl/lppl-1-0.html
```

## `uk`, from `hyph-uk.tex`

Taken under: MIT.

```text
title: Hyphenation patterns for Ukrainian
copyright: Copyright (C) 1998-2001 Maksym Polyakov
notice: This file is part of the hyph-utf8 package.
    See http://www.hyphenation.org/tex for more information.
version: 2001/05/10
authors:
  -
    name: Maksym Polyakov
    contact: polyama (at) auburn.edu, mpoliak (at) i.com.ua
licence:
    - This file is available under any of the following licences:
    -
        name: MIT
        url: https://opensource.org/licenses/MIT
    -
        name: LPPL
        url: https://latex-project.org/lppl/
```
