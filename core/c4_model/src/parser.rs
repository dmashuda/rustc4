//! Tokens to an [`ast::File`]. Recursive descent; an error skips to the next
//! line and carries on, so one typo does not hide the rest of the file.

use crate::ast::*;
use crate::lexer::{Tok, Token, lex};

pub struct ParseError {
    pub span: Span,
    pub message: String,
}

pub fn parse(file: usize, text: &str) -> (File, Vec<ParseError>) {
    let (tokens, lex_errors) = lex(file, text);
    let mut p = Parser {
        tokens,
        at: 0,
        errors: lex_errors.into_iter().map(|e| ParseError { span: e.span, message: e.message }).collect(),
        out: File::default(),
    };
    p.file();
    p.errors.sort_by_key(|e| (e.span.line, e.span.col));
    (p.out, p.errors)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    errors: Vec<ParseError>,
    out: File,
}

/// An error already recorded; the caller only has to recover.
struct Failed;
type Res<T> = Result<T, Failed>;

const STRING_PROPS: [&str; 5] = ["title", "description", "technology", "summary", "notation"];

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.at].tok
    }

    fn peek_at(&self, n: usize) -> &Tok {
        &self.tokens[(self.at + n).min(self.tokens.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.tokens[self.at].span
    }

    fn next(&mut self) -> Tok {
        let tok = self.tokens[self.at].tok.clone();
        if self.at < self.tokens.len() - 1 {
            self.at += 1;
        }
        tok
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.next();
            true
        } else {
            false
        }
    }

    fn is_word(&self, word: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == word)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.is_word(word) {
            self.next();
            true
        } else {
            false
        }
    }

    fn fail<T>(&mut self, span: Span, message: impl Into<String>) -> Res<T> {
        self.errors.push(ParseError { span, message: message.into() });
        Err(Failed)
    }

    fn unexpected<T>(&mut self, wanted: &str) -> Res<T> {
        let found = self.peek().describe();
        self.fail(self.span(), format!("Expected {wanted}, found {found}."))
    }

    fn expect(&mut self, tok: Tok) -> Res<()> {
        if self.eat(&tok) { Ok(()) } else { self.unexpected(&tok.describe()) }
    }

    fn ident(&mut self, what: &str) -> Res<String> {
        match self.peek() {
            Tok::Ident(_) => match self.next() {
                Tok::Ident(s) => Ok(s),
                _ => unreachable!(),
            },
            _ => self.unexpected(what),
        }
    }

    fn string(&mut self, what: &str) -> Res<String> {
        match self.peek() {
            Tok::Str(_) => match self.next() {
                Tok::Str(s) => Ok(s),
                _ => unreachable!(),
            },
            _ => self.unexpected(what),
        }
    }

    fn opt_string(&mut self) -> Option<String> {
        match self.peek() {
            Tok::Str(_) => match self.next() {
                Tok::Str(s) => Some(s),
                _ => unreachable!(),
            },
            _ => None,
        }
    }

    /// After an error: on to the first token of a later line, stepping over
    /// any `{ ... }` on the way, and stopping short of a `}` that closes the
    /// block the error was in.
    fn recover(&mut self, from_line: u32) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                Tok::Eof => return,
                Tok::LBrace => depth += 1,
                Tok::RBrace if depth == 0 => return,
                Tok::RBrace => depth -= 1,
                _ if depth == 0 && self.span().line > from_line => return,
                _ => {}
            }
            self.next();
        }
    }

    /// `{ item* }`, each item recovering on its own.
    fn block(&mut self, mut item: impl FnMut(&mut Parser) -> Res<()>) -> Res<()> {
        self.expect(Tok::LBrace)?;
        loop {
            while self.eat(&Tok::Semi) || self.eat(&Tok::Comma) {}
            if self.eat(&Tok::RBrace) {
                return Ok(());
            }
            if *self.peek() == Tok::Eof {
                return self.unexpected("'}'");
            }
            let (line, before) = (self.span().line, self.at);
            if item(self).is_err() {
                self.recover(line);
                // Always move, or an item that fails without consuming
                // anything would be tried forever.
                if self.at == before && *self.peek() != Tok::RBrace {
                    self.next();
                }
            }
        }
    }

    fn skip_block(&mut self) -> Res<()> {
        self.expect(Tok::LBrace)?;
        let mut depth = 1;
        while depth > 0 {
            match self.next() {
                Tok::LBrace => depth += 1,
                Tok::RBrace => depth -= 1,
                Tok::Eof => return self.unexpected("'}'"),
                _ => {}
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ file --

    fn file(&mut self) {
        loop {
            let span = self.span();
            let result = match self.peek().clone() {
                Tok::Eof => return,
                Tok::Ident(word) => match word.as_str() {
                    "specification" => {
                        self.next();
                        self.block(Parser::spec_item)
                    }
                    "model" => {
                        self.next();
                        let mut items = Vec::new();
                        let r = self.block(|p| p.model_item(&mut items));
                        self.out.model.append(&mut items);
                        r
                    }
                    "views" => {
                        self.next();
                        self.opt_string();
                        self.block(Parser::views_item)
                    }
                    "global" | "deployment" | "likec4lib" | "import" => {
                        self.next();
                        self.errors.push(ParseError {
                            span,
                            message: format!("'{word}' blocks are not supported yet; this one is ignored."),
                        });
                        while !matches!(self.peek(), Tok::LBrace | Tok::Eof) {
                            self.next();
                        }
                        self.skip_block()
                    }
                    _ => self.unexpected("'specification', 'model' or 'views'"),
                },
                _ => self.unexpected("'specification', 'model' or 'views'"),
            };
            if result.is_err() {
                let before = self.at;
                self.recover(span.line);
                if self.at == before {
                    self.next();
                }
            }
        }
    }

    // --------------------------------------------------- specification --

    fn spec_item(&mut self) -> Res<()> {
        let span = self.span();
        let word = self.ident("'element', 'relationship', 'tag' or 'color'")?;
        match word.as_str() {
            "element" | "relationship" => {
                let name = self.ident("a name for this kind")?;
                let mut style = StyleProps::default();
                let mut technology = None;
                if *self.peek() == Tok::LBrace {
                    self.block(|p| {
                        if p.is_word("style") && *p.peek_at(1) == Tok::LBrace {
                            p.next();
                            return p.style_block(&mut style);
                        }
                        if let Tok::Ident(key) = p.peek().clone()
                            && STRING_PROPS.contains(&key.as_str())
                        {
                            p.next();
                            p.eat(&Tok::Colon);
                            let value = p.string("a string")?;
                            if key == "technology" {
                                technology = Some(value);
                            }
                            return Ok(());
                        }
                        // A relationship kind takes its style without the
                        // `style { }` around it.
                        p.style_prop(&mut style)
                    })?;
                }
                self.out.specs.push(if word == "element" {
                    SpecItem::Element { name, span, style, technology }
                } else {
                    SpecItem::Relationship { name, span, style, technology }
                });
                Ok(())
            }
            "tag" => {
                self.ident("a tag name")?;
                if *self.peek() == Tok::LBrace {
                    self.skip_block()?;
                }
                self.out.specs.push(SpecItem::Tag);
                Ok(())
            }
            "color" => {
                let name = self.ident("a color name")?;
                let value = self.value()?;
                self.out.specs.push(SpecItem::Color { name, value, span });
                Ok(())
            }
            _ => self
                .fail(span, format!("Expected 'element', 'relationship', 'tag' or 'color', found '{word}'.")),
        }
    }

    fn style_block(&mut self, style: &mut StyleProps) -> Res<()> {
        self.block(|p| p.style_prop(style))
    }

    /// `key value` or `key: value`.
    fn style_prop(&mut self, style: &mut StyleProps) -> Res<()> {
        let span = self.span();
        let key = self.ident("a style property")?;
        self.eat(&Tok::Colon);
        let value = self.value()?;
        style.0.push((key, value, span));
        Ok(())
    }

    /// One value: `person`, `10%`, `#ff00aa`, `tech:nextjs`, `'text'`.
    fn value(&mut self) -> Res<String> {
        match self.next() {
            Tok::Ident(mut s) => {
                // `tech:nextjs`
                while *self.peek() == Tok::Colon && matches!(self.peek_at(1), Tok::Ident(_)) {
                    self.next();
                    if let Tok::Ident(more) = self.next() {
                        s.push(':');
                        s.push_str(&more);
                    }
                }
                Ok(s)
            }
            Tok::Num(s) | Tok::Str(s) | Tok::Raw(s) => Ok(s),
            Tok::Hash(s) => Ok(format!("#{s}")),
            _ => {
                self.at -= 1;
                self.unexpected("a value")
            }
        }
    }

    // ----------------------------------------------------------- model --

    fn reference(&mut self) -> Res<Ref> {
        let span = self.span();
        let mut parts = vec![self.ident("an element name")?];
        while *self.peek() == Tok::Dot && matches!(self.peek_at(1), Tok::Ident(_)) {
            self.next();
            parts.push(self.ident("an element name")?);
        }
        Ok(Ref { parts, span })
    }

    fn tags(&mut self, tags: &mut Vec<String>) {
        while let Tok::Hash(name) = self.peek().clone() {
            self.next();
            tags.push(name);
            self.eat(&Tok::Comma);
        }
    }

    fn model_item(&mut self, items: &mut Vec<ModelItem>) -> Res<()> {
        match self.peek().clone() {
            Tok::Arrow | Tok::ArrowOpen => {
                items.push(ModelItem::Relation(self.relation(None)?));
                Ok(())
            }
            Tok::Ident(word) => {
                if word == "extend" {
                    self.next();
                    let target = self.reference()?;
                    let mut inner = Vec::new();
                    self.block(|p| p.model_item(&mut inner))?;
                    items.push(ModelItem::Extend { target, items: inner });
                    return Ok(());
                }
                match self.peek_at(1) {
                    Tok::Eq => {
                        // name = kind 'Title'
                        let span = self.span();
                        self.next();
                        self.next();
                        let kind = self.ident("an element kind")?;
                        items.push(ModelItem::Element(self.element(word, kind, span)?));
                    }
                    Tok::Ident(_) => {
                        // kind name 'Title'
                        self.next();
                        let span = self.span();
                        let name = self.ident("an element name")?;
                        items.push(ModelItem::Element(self.element(name, word, span)?));
                    }
                    Tok::Arrow | Tok::ArrowOpen | Tok::Dot => {
                        let source = self.reference()?;
                        let implicit =
                            source.parts.len() == 1 && matches!(source.parts[0].as_str(), "this" | "it");
                        items.push(ModelItem::Relation(self.relation(if implicit {
                            None
                        } else {
                            Some(source)
                        })?));
                    }
                    _ => {
                        self.next();
                        return self.unexpected(&format!(
                            "'=' or '->' after '{word}' (an element is `name = kind 'Title'`, a relationship `a -> b`)"
                        ));
                    }
                }
                Ok(())
            }
            _ => self.unexpected("an element or a relationship"),
        }
    }

    fn element(&mut self, name: String, kind: String, span: Span) -> Res<ElementDef> {
        let mut el = ElementDef { name, kind, span, ..Default::default() };
        // Up to three strings: title, description, technology.
        el.title = self.opt_string();
        if el.title.is_some() {
            self.eat(&Tok::Comma);
            el.description = self.opt_string();
            if el.description.is_some() {
                self.eat(&Tok::Comma);
                el.technology = self.opt_string();
            }
        }
        if *self.peek() == Tok::LBrace {
            self.block(|p| p.element_item(&mut el))?;
        }
        Ok(el)
    }

    fn element_item(&mut self, el: &mut ElementDef) -> Res<()> {
        if matches!(self.peek(), Tok::Hash(_)) {
            self.tags(&mut el.tags);
            return Ok(());
        }
        if let Tok::Ident(word) = self.peek().clone() {
            let after = self.peek_at(1).clone();
            let prop_value = matches!(after, Tok::Str(_))
                || (after == Tok::Colon && matches!(self.peek_at(2), Tok::Str(_)));
            if STRING_PROPS.contains(&word.as_str()) && prop_value {
                self.next();
                self.eat(&Tok::Colon);
                let value = self.string("a string")?;
                match word.as_str() {
                    "title" => el.title = Some(value),
                    "description" => el.description = Some(value),
                    "summary" if el.description.is_none() => el.description = Some(value),
                    "technology" => el.technology = Some(value),
                    _ => {}
                }
                return Ok(());
            }
            match (word.as_str(), &after) {
                ("style", Tok::LBrace) => {
                    self.next();
                    return self.style_block(&mut el.style);
                }
                ("metadata", Tok::LBrace) => {
                    self.next();
                    return self.block(|p| {
                        let key = p.ident("a metadata key")?;
                        p.eat(&Tok::Colon);
                        let value = p.value()?;
                        el.metadata.push((key, value));
                        Ok(())
                    });
                }
                ("link", Tok::Raw(_) | Tok::Str(_)) => {
                    self.next();
                    let url = self.value()?;
                    let title = self.opt_string();
                    el.links.push((url, title));
                    return Ok(());
                }
                ("icon", Tok::Ident(_) | Tok::Raw(_) | Tok::Str(_))
                    if !matches!(self.peek_at(2), Tok::Eq) =>
                {
                    let span = self.span();
                    self.next();
                    let value = self.value()?;
                    el.style.0.push(("icon".into(), value, span));
                    return Ok(());
                }
                _ => {}
            }
        }
        self.model_item(&mut el.items)
    }

    /// From the arrow on: `-> target 'title' 'technology' #tags { ... }`.
    fn relation(&mut self, source: Option<Ref>) -> Res<RelationDef> {
        let span = source.as_ref().map(|s| s.span).unwrap_or(self.span());
        let kind = match self.next() {
            Tok::Arrow => None,
            Tok::ArrowOpen => {
                let kind = self.ident("a relationship kind")?;
                self.expect(Tok::ArrowClose)?;
                Some(kind)
            }
            Tok::Dot => {
                // `a .uses b`
                Some(self.ident("a relationship kind")?)
            }
            _ => {
                self.at -= 1;
                return self.unexpected("'->'");
            }
        };
        let target = self.reference()?;
        let mut rel = RelationDef {
            source,
            target,
            kind,
            span,
            title: None,
            description: None,
            technology: None,
            tags: Vec::new(),
            style: StyleProps::default(),
        };
        rel.title = self.opt_string();
        if rel.title.is_some() {
            self.eat(&Tok::Comma);
            rel.technology = self.opt_string();
        }
        self.tags(&mut rel.tags);
        if *self.peek() == Tok::LBrace {
            self.block(|p| {
                if matches!(p.peek(), Tok::Hash(_)) {
                    p.tags(&mut rel.tags);
                    return Ok(());
                }
                let word = match p.peek() {
                    Tok::Ident(word) => word.clone(),
                    _ => return p.unexpected("a relationship property"),
                };
                match word.as_str() {
                    "style" if *p.peek_at(1) == Tok::LBrace => {
                        p.next();
                        p.style_block(&mut rel.style)
                    }
                    "metadata" if *p.peek_at(1) == Tok::LBrace => {
                        p.next();
                        p.skip_block()
                    }
                    "link" => {
                        p.next();
                        p.value()?;
                        p.opt_string();
                        Ok(())
                    }
                    _ if STRING_PROPS.contains(&word.as_str()) => {
                        p.next();
                        p.eat(&Tok::Colon);
                        let value = p.string("a string")?;
                        match word.as_str() {
                            "title" => rel.title = Some(value),
                            "description" => rel.description = Some(value),
                            "technology" => rel.technology = Some(value),
                            _ => {}
                        }
                        Ok(())
                    }
                    // `line dotted`, `color red`, `head diamond`, `navigateTo view`
                    _ => p.style_prop(&mut rel.style),
                }
            })?;
        }
        Ok(rel)
    }

    // ----------------------------------------------------------- views --

    fn views_item(&mut self) -> Res<()> {
        let span = self.span();
        if self.eat_word("style") {
            let rule = self.style_rule()?;
            self.out.view_styles.push(rule);
            return Ok(());
        }
        if self.is_word("deployment") {
            self.next();
            self.errors.push(ParseError {
                span,
                message: "deployment views are not supported yet; this one is ignored.".into(),
            });
            while !matches!(self.peek(), Tok::LBrace | Tok::Eof) {
                self.next();
            }
            return self.skip_block();
        }
        let dynamic = self.eat_word("dynamic");
        if !self.eat_word("view") {
            return self.unexpected("'view'");
        }
        let mut view = ViewDef {
            id: None,
            of: None,
            extends: None,
            span,
            title: None,
            description: None,
            tags: Vec::new(),
            rules: Vec::new(),
            dynamic,
            sequence: false,
            steps: Vec::new(),
        };
        if dynamic {
            view.id = Some(self.ident("a name for this dynamic view")?);
            self.block(|p| p.dynamic_item(&mut view))?;
            self.out.views.push(view);
            return Ok(());
        }
        if !(self.is_word("of") || self.is_word("extends")) && matches!(self.peek(), Tok::Ident(_)) {
            view.id = Some(self.ident("a view name")?);
        }
        if self.eat_word("of") {
            view.of = Some(self.reference()?);
        } else if self.eat_word("extends") {
            let at = self.span();
            view.extends = Some((self.ident("the view to extend")?, at));
        }
        self.block(|p| p.view_item(&mut view))?;
        self.out.views.push(view);
        Ok(())
    }

    fn view_item(&mut self, view: &mut ViewDef) -> Res<()> {
        if matches!(self.peek(), Tok::Hash(_)) {
            self.tags(&mut view.tags);
            return Ok(());
        }
        let span = self.span();
        let word = self.ident("'include', 'exclude', 'title', 'style' or 'autoLayout'")?;
        match word.as_str() {
            "title" | "description" => {
                self.eat(&Tok::Colon);
                let value = self.string("a string")?;
                if word == "title" {
                    view.title = Some(value);
                } else {
                    view.description = Some(value);
                }
            }
            "link" => {
                self.value()?;
                self.opt_string();
            }
            "include" => {
                let predicates = self.predicates()?;
                view.rules.push(ViewRule::Include(predicates));
            }
            "exclude" => {
                let predicates = self.predicates()?;
                view.rules.push(ViewRule::Exclude(predicates));
            }
            "style" => {
                let rule = self.style_rule()?;
                view.rules.push(rule);
            }
            "autoLayout" => {
                let at = self.span();
                let dir = self.ident("a direction")?;
                let direction = match dir.as_str() {
                    "TopBottom" => Direction::TopBottom,
                    "BottomTop" => Direction::BottomTop,
                    "LeftRight" => Direction::LeftRight,
                    "RightLeft" => Direction::RightLeft,
                    _ => {
                        return self.fail(
                            at,
                            format!(
                                "Unknown direction '{dir}': use TopBottom, BottomTop, LeftRight or RightLeft."
                            ),
                        );
                    }
                };
                // Optional rank and node separation, which the layout here
                // picks for itself.
                while matches!(self.peek(), Tok::Num(_)) {
                    self.next();
                }
                view.rules.push(ViewRule::AutoLayout(direction));
            }
            _ => {
                return self.fail(
                    span,
                    format!(
                        "Expected 'include', 'exclude', 'title', 'style' or 'autoLayout', found '{word}'."
                    ),
                );
            }
        }
        Ok(())
    }

    fn starts_step(&self) -> bool {
        matches!(self.peek(), Tok::Ident(_))
            && matches!(self.peek_at(1), Tok::Arrow | Tok::BackArrow | Tok::ArrowOpen | Tok::Dot)
    }

    /// What a dynamic view holds: steps, and the same `title`, `include`,
    /// `style` and `autoLayout` as any view.
    fn dynamic_item(&mut self, view: &mut ViewDef) -> Res<()> {
        if self.starts_step() {
            let mut steps = Vec::new();
            let result = self.steps(&mut steps);
            view.steps.extend(steps.into_iter().map(StepItem::One));
            return result;
        }
        if (self.is_word("parallel") || self.is_word("par")) && *self.peek_at(1) == Tok::LBrace {
            self.next();
            let mut steps = Vec::new();
            let result = self.block(|p| {
                if p.starts_step() { p.steps(&mut steps) } else { p.unexpected("a step, like 'a -> b'") }
            });
            view.steps.push(StepItem::Parallel(steps));
            return result;
        }
        if self.is_word("variant") {
            self.next();
            let at = self.span();
            let variant = self.ident("'diagram' or 'sequence'")?;
            view.sequence = match variant.as_str() {
                "sequence" => true,
                "diagram" => false,
                _ => return self.fail(at, format!("Unknown variant '{variant}': use diagram or sequence.")),
            };
            return Ok(());
        }
        self.view_item(view)
    }

    /// `a -> b 'title'`, and on along a chain: `a -> b -> c`.
    fn steps(&mut self, out: &mut Vec<StepDef>) -> Res<()> {
        let mut from = self.reference()?;
        loop {
            let span = from.span;
            let (kind, backward) = match self.next() {
                Tok::Arrow => (None, false),
                Tok::BackArrow => (None, true),
                Tok::ArrowOpen => {
                    let kind = self.ident("a relationship kind")?;
                    self.expect(Tok::ArrowClose)?;
                    (Some(kind), false)
                }
                _ => {
                    self.at -= 1;
                    return self.unexpected("'->' or '<-'");
                }
            };
            let to = self.reference()?;
            let (source, target) = if backward { (to.clone(), from) } else { (from, to.clone()) };
            let mut step = StepDef {
                source,
                target,
                backward,
                kind,
                span,
                title: None,
                technology: None,
                notes: None,
                style: StyleProps::default(),
            };
            step.title = self.opt_string();
            if step.title.is_some() {
                self.eat(&Tok::Comma);
                step.technology = self.opt_string();
            }
            let mut tags = Vec::new();
            self.tags(&mut tags);
            if *self.peek() == Tok::LBrace {
                self.block(|p| {
                    if matches!(p.peek(), Tok::Hash(_)) {
                        p.tags(&mut tags);
                        return Ok(());
                    }
                    let word = match p.peek() {
                        Tok::Ident(word) => word.clone(),
                        _ => return p.unexpected("a step property"),
                    };
                    match word.as_str() {
                        "style" if *p.peek_at(1) == Tok::LBrace => {
                            p.next();
                            p.style_block(&mut step.style)
                        }
                        "metadata" if *p.peek_at(1) == Tok::LBrace => {
                            p.next();
                            p.skip_block()
                        }
                        "title" | "description" | "technology" | "notes" | "notation" => {
                            p.next();
                            p.eat(&Tok::Colon);
                            let value = p.string("a string")?;
                            match word.as_str() {
                                "title" => step.title = Some(value),
                                "technology" => step.technology = Some(value),
                                "notes" => step.notes = Some(value),
                                "description" if step.notes.is_none() => step.notes = Some(value),
                                _ => {}
                            }
                            Ok(())
                        }
                        _ => p.style_prop(&mut step.style),
                    }
                })?;
            }
            out.push(step);
            if !matches!(self.peek(), Tok::Arrow | Tok::BackArrow | Tok::ArrowOpen) {
                return Ok(());
            }
            from = to;
        }
    }

    /// `style a, b.* { color muted }`
    fn style_rule(&mut self) -> Res<ViewRule> {
        let mut targets = vec![self.element_expr()?];
        while self.eat(&Tok::Comma) {
            if *self.peek() == Tok::LBrace {
                break;
            }
            targets.push(self.element_expr()?);
        }
        let mut style = StyleProps::default();
        self.style_block(&mut style)?;
        Ok(ViewRule::Style { targets, style })
    }

    fn predicates(&mut self) -> Res<Vec<Predicate>> {
        let mut out = vec![self.predicate()?];
        while self.eat(&Tok::Comma) {
            // A trailing comma is fine: what follows is the next rule.
            let starts_predicate = match self.peek() {
                Tok::Star | Tok::Arrow => true,
                Tok::Ident(word) => {
                    !matches!(
                        word.as_str(),
                        "include" | "exclude" | "style" | "autoLayout" | "title" | "description"
                    ) || *self.peek_at(1) == Tok::Dot
                }
                _ => false,
            };
            if !starts_predicate {
                break;
            }
            out.push(self.predicate()?);
        }
        Ok(out)
    }

    fn predicate(&mut self) -> Res<Predicate> {
        if self.eat(&Tok::Arrow) {
            let expr = self.element_expr()?;
            return Ok(Predicate::Relation(if self.eat(&Tok::Arrow) {
                RelExpr::InOut(expr)
            } else {
                RelExpr::Incoming(expr)
            }));
        }
        let expr = self.element_expr()?;
        if self.eat(&Tok::Arrow) || self.eat(&Tok::BiArrow) {
            let starts_expr = match self.peek() {
                Tok::Star => true,
                Tok::Ident(word) => !matches!(word.as_str(), "include" | "exclude" | "style" | "autoLayout"),
                _ => false,
            };
            // `a ->` ends at the end of the line.
            let same_line = self.tokens[self.at].span.line == self.tokens[self.at - 1].span.line;
            return Ok(Predicate::Relation(if starts_expr && same_line {
                RelExpr::Direct(expr, self.element_expr()?)
            } else {
                RelExpr::Outgoing(expr)
            }));
        }
        let mut with = StyleProps::default();
        if self.is_word("where") {
            return self.fail(self.span(), "'where' conditions are not supported yet.");
        }
        if self.eat_word("with") {
            self.block(|p| p.style_prop(&mut with))?;
        }
        Ok(Predicate::Element { expr, with })
    }

    fn element_expr(&mut self) -> Res<ElementExpr> {
        if self.eat(&Tok::Star) {
            return Ok(ElementExpr::Wildcard);
        }
        // element.kind = x, element.tag != #x
        if self.is_word("element")
            && *self.peek_at(1) == Tok::Dot
            && matches!(self.peek_at(2), Tok::Ident(w) if w == "kind" || w == "tag")
            && matches!(self.peek_at(3), Tok::Eq | Tok::Ne)
        {
            self.next();
            self.next();
            let what = self.ident("'kind' or 'tag'")?;
            let eq = self.next() == Tok::Eq;
            return Ok(if what == "kind" {
                ElementExpr::Kind { name: self.ident("an element kind")?, eq }
            } else {
                match self.next() {
                    Tok::Hash(name) => ElementExpr::Tag { name, eq },
                    _ => {
                        self.at -= 1;
                        return self.unexpected("a tag, like '#next'");
                    }
                }
            });
        }
        let reference = self.reference()?;
        if *self.peek() == Tok::Dot {
            if *self.peek_at(1) == Tok::Star {
                self.next();
                self.next();
                // `.**` is two stars with nothing between them.
                if *self.peek() == Tok::Star && !self.tokens[self.at].spaced {
                    self.next();
                    return Ok(ElementExpr::Descendants(reference));
                }
                return Ok(ElementExpr::Children(reference));
            }
            return self.unexpected("an element name or '*' after '.'");
        }
        // `a._`: lexed as part of the name.
        if reference.parts.len() > 1 && reference.parts.last().is_some_and(|p| p == "_") {
            let mut parts = reference.parts.clone();
            parts.pop();
            return Ok(ElementExpr::Expanded(Ref { parts, span: reference.span }));
        }
        Ok(ElementExpr::Ref(reference))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> File {
        let (file, errors) = parse(0, text);
        let messages: Vec<_> =
            errors.iter().map(|e| format!("{}:{} {}", e.span.line, e.span.col, e.message)).collect();
        assert!(messages.is_empty(), "{messages:?}");
        file
    }

    #[test]
    fn elements_in_both_spellings() {
        let file = ok("model {
            customer = actor 'Customer' 'Buys things'
            system cloud 'Cloud' {
                description 'Where it runs'
                #core, #next
                link https://example.com 'Docs'
                style { shape browser; color: green }
                api = service 'API' 'Serves' 'Rust'
                -> customer 'emails'
                api -[async]-> customer 'notifies' 'SMTP' { line dotted }
            }
        }");
        assert_eq!(file.model.len(), 2);
        let ModelItem::Element(cloud) = &file.model[1] else { panic!() };
        assert_eq!((cloud.kind.as_str(), cloud.name.as_str()), ("system", "cloud"));
        assert_eq!(cloud.description.as_deref(), Some("Where it runs"));
        assert_eq!(cloud.tags, ["core", "next"]);
        assert_eq!(cloud.links[0].0, "https://example.com");
        assert_eq!(cloud.style.0.len(), 2);
        assert_eq!(cloud.items.len(), 3);
        let ModelItem::Element(api) = &cloud.items[0] else { panic!() };
        assert_eq!(api.technology.as_deref(), Some("Rust"));
        let ModelItem::Relation(implicit) = &cloud.items[1] else { panic!() };
        assert!(implicit.source.is_none());
        let ModelItem::Relation(rel) = &cloud.items[2] else { panic!() };
        assert_eq!(rel.kind.as_deref(), Some("async"));
        assert_eq!(rel.technology.as_deref(), Some("SMTP"));
        assert_eq!(rel.style.0[0].0, "line");
    }

    #[test]
    fn views_and_predicates() {
        let file = ok("views {
            view index {
                title 'Landscape'
                include *
            }
            view of cloud.api {
                include *, cloud.*, -> cloud.api ->, a -> b,
                exclude element.kind = actor, cloud.db.**
                include customer with { color red }
                autoLayout LeftRight
                style *, cloud._ { color muted; opacity 10% }
            }
        }");
        assert_eq!(file.views.len(), 2);
        let view = &file.views[1];
        assert!(view.id.is_none());
        assert_eq!(view.of.as_ref().unwrap().text(), "cloud.api");
        let ViewRule::Include(first) = &view.rules[0] else { panic!() };
        assert_eq!(first.len(), 4);
        assert!(matches!(first[2], Predicate::Relation(RelExpr::InOut(_))));
        assert!(matches!(first[3], Predicate::Relation(RelExpr::Direct(..))));
        let ViewRule::Exclude(second) = &view.rules[1] else { panic!() };
        assert!(matches!(&second[0], Predicate::Element { expr: ElementExpr::Kind { eq: true, .. }, .. }));
        assert!(matches!(&second[1], Predicate::Element { expr: ElementExpr::Descendants(_), .. }));
        assert!(matches!(view.rules[3], ViewRule::AutoLayout(Direction::LeftRight)));
        let ViewRule::Style { targets, style } = &view.rules[4] else { panic!() };
        assert_eq!((targets.len(), style.0.len()), (2, 2));
    }

    #[test]
    fn an_error_does_not_hide_the_rest() {
        let (file, errors) = parse(
            0,
            "model {
                a = system 'A'
                b = = system
                c = system 'C'
            }
            views { view v { include * } }",
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].span.line, 3);
        assert_eq!(file.model.len(), 2);
        assert_eq!(file.views.len(), 1);
    }
}
