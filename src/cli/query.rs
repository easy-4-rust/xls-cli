//! A small read-only SQL engine over workbook sheets.
//!
//! Each sheet is a table named by its sheet name; **row 0 is the header** and
//! supplies column names (columns are also addressable by their letter, `A`,
//! `B`, …). Supports a useful subset of `SELECT`:
//!
//! ```text
//! SELECT * | col, AGG(col) [AS alias], …
//! FROM sheet [alias]
//! [[INNER] JOIN sheet [alias] ON a.col = b.col]      -- equi-join only
//! [WHERE cond]                                       -- = != <> < <= > >= LIKE, AND/OR/NOT, ( )
//! [GROUP BY col, …]                                  -- with SUM/COUNT/AVG/MIN/MAX
//! [ORDER BY col|alias|ordinal [ASC|DESC], …]
//! [LIMIT n]
//! ```
//!
//! It is deliberately small and forgiving (e.g. a non-grouped column selected
//! alongside aggregates takes the first row of each group, like `SQLite`). It is
//! a query layer only — it never mutates the workbook.

use std::cmp::Ordering::{Equal, Greater, Less};
use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};

use easyexcel::model::Workbook;
use easyexcel::model::addr::col_letters_to_index;
use easyexcel::model::value::parse_number_text;
use easyexcel::model::value::{CellValue, format_number_general};

/// The result of a query: column headers plus row-major scalar data.
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<CellValue>>,
}

/// Parse and run a `SELECT` query against the workbook's sheets.
pub fn run_query(wb: &Workbook, sql: &str) -> Result<QueryResult> {
    let toks = tokenize(sql)?;
    let stmt = Parser::new(toks).parse_select()?;
    execute(wb, &stmt)
}

// ─── Tokens ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// A bare word. Keywords (SELECT, FROM, …) are *not* reserved at the token
    /// level — they are recognized contextually by the parser — so a column may
    /// legitimately be named `desc`, `order`, etc.
    Ident(String),
    Str(String),
    Num(f64),
    // punctuation / operators
    Star,
    Comma,
    Dot,
    LParen,
    RParen,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[allow(
    clippy::too_many_lines,
    reason = "词法分支保持在单个线性扫描中便于核对 token 消费和游标推进"
)]
fn tokenize(s: &str) -> Result<Vec<Tok>> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '=' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                }
                out.push(Tok::Eq);
            }
            '!' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    out.push(Tok::Ne);
                } else {
                    bail!("unexpected '!' in query");
                }
            }
            '<' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    out.push(Tok::Le);
                } else if i < chars.len() && chars[i] == '>' {
                    i += 1;
                    out.push(Tok::Ne);
                } else {
                    out.push(Tok::Lt);
                }
            }
            '>' => {
                i += 1;
                if i < chars.len() && chars[i] == '=' {
                    i += 1;
                    out.push(Tok::Ge);
                } else {
                    out.push(Tok::Gt);
                }
            }
            '\'' => {
                // string literal; '' is an escaped quote
                i += 1;
                let mut buf = String::new();
                loop {
                    if i >= chars.len() {
                        bail!("unterminated string literal");
                    }
                    if chars[i] == '\'' {
                        if i + 1 < chars.len() && chars[i + 1] == '\'' {
                            buf.push('\'');
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    } else {
                        buf.push(chars[i]);
                        i += 1;
                    }
                }
                out.push(Tok::Str(buf));
            }
            '"' | '`' | '[' => {
                // quoted identifier (allows spaces in sheet/column names)
                let close = if c == '[' { ']' } else { c };
                i += 1;
                let mut buf = String::new();
                loop {
                    if i >= chars.len() {
                        bail!("unterminated quoted identifier");
                    }
                    if chars[i] == close {
                        i += 1;
                        break;
                    }
                    buf.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Ident(buf));
            }
            '.' => {
                // a dot starting a number, else the member operator
                if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                    let (tok, ni) = read_number(&chars, i)?;
                    out.push(tok);
                    i = ni;
                } else {
                    out.push(Tok::Dot);
                    i += 1;
                }
            }
            d if d.is_ascii_digit() => {
                let (tok, ni) = read_number(&chars, i)?;
                out.push(tok);
                i = ni;
            }
            a if a.is_alphabetic() || a == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                out.push(Tok::Ident(word));
            }
            other => bail!("unexpected character '{other}' in query"),
        }
    }
    Ok(out)
}

fn read_number(chars: &[char], start: usize) -> Result<(Tok, usize)> {
    let mut i = start;
    while i < chars.len()
        && (chars[i].is_ascii_digit()
            || chars[i] == '.'
            || chars[i] == 'e'
            || chars[i] == 'E'
            || ((chars[i] == '+' || chars[i] == '-')
                && i > start
                && matches!(chars[i - 1], 'e' | 'E')))
    {
        i += 1;
    }
    let s: String = chars[start..i].iter().collect();
    let n = s
        .parse::<f64>()
        .map_err(|_| anyhow!("invalid number '{s}'"))?;
    Ok((Tok::Num(n), i))
}

/// Words that begin a clause — never treated as a bare table/column alias.
const RESERVED: &[&str] = &[
    "FROM", "WHERE", "GROUP", "ORDER", "LIMIT", "JOIN", "INNER", "ON", "AND", "OR", "ASC", "DESC",
    "BY", "LIKE", "AS", "NOT", "SELECT",
];

fn is_reserved(word: &str) -> bool {
    RESERVED.iter().any(|k| k.eq_ignore_ascii_case(word))
}

// ─── AST ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum AggFn {
    Sum,
    Count,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone)]
struct ColRef {
    table: Option<String>,
    name: String,
}

#[derive(Debug, Clone)]
enum SelExpr {
    Col(ColRef),
    Agg(AggFn, Option<ColRef>), // None arg == COUNT(*)
}

#[derive(Debug, Clone)]
enum SelectItem {
    Star,
    Expr {
        expr: SelExpr,
        alias: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct TableRef {
    name: String,
    alias: Option<String>,
}

#[derive(Debug, Clone)]
struct Join {
    table: TableRef,
    left: ColRef,
    right: ColRef,
}

#[derive(Debug, Clone)]
enum Operand {
    Col(ColRef),
    Num(f64),
    Str(String),
}

#[derive(Debug, Clone, Copy)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
enum Cond {
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
    Cmp(Operand, CmpOp, Operand),
    Like(Operand, String),
}

#[derive(Debug, Clone)]
struct OrderKey {
    /// 1-based ordinal, or a column/alias name.
    by: OrderBy,
    desc: bool,
}

#[derive(Debug, Clone)]
enum OrderBy {
    Ordinal(usize),
    Name(String),
}

#[derive(Debug, Clone)]
struct SelectStmt {
    items: Vec<SelectItem>,
    from: TableRef,
    join: Option<Join>,
    where_: Option<Cond>,
    group_by: Vec<ColRef>,
    order_by: Vec<OrderKey>,
    limit: Option<usize>,
}

// ─── Parser ──────────────────────────────────────────────────────────────────

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn new(toks: Vec<Tok>) -> Self {
        Parser { toks, pos: 0 }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn peek2(&self) -> Option<&Tok> {
        self.toks.get(self.pos + 1)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &Tok) -> Result<()> {
        if self.eat(t) {
            Ok(())
        } else {
            bail!("expected {t:?}, found {:?}", self.peek())
        }
    }
    fn expect_ident(&mut self) -> Result<String> {
        match self.next() {
            Some(Tok::Ident(s)) => Ok(s),
            other => bail!("expected an identifier, found {other:?}"),
        }
    }

    /// True if the next token is the keyword `kw` (case-insensitive).
    fn peek_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s.eq_ignore_ascii_case(kw))
    }
    fn peek2_kw(&self, kw: &str) -> bool {
        matches!(self.peek2(), Some(Tok::Ident(s)) if s.eq_ignore_ascii_case(kw))
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.peek_kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect_kw(&mut self, kw: &str) -> Result<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            bail!("expected `{kw}`, found {:?}", self.peek())
        }
    }
    /// True if the next token is a bare identifier usable as an alias (i.e. an
    /// `Ident` that is not a clause keyword).
    fn peek_plain_ident(&self) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if !is_reserved(s))
    }

    fn parse_select(&mut self) -> Result<SelectStmt> {
        self.expect_kw("SELECT")?;
        let mut items = Vec::new();
        loop {
            items.push(self.parse_select_item()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect_kw("FROM")?;
        let from = self.parse_table_ref()?;

        let mut join = None;
        let is_join = self.peek_kw("JOIN") || (self.peek_kw("INNER") && self.peek2_kw("JOIN"));
        if is_join {
            self.eat_kw("INNER");
            self.expect_kw("JOIN")?;
            let table = self.parse_table_ref()?;
            self.expect_kw("ON")?;
            let left = self.parse_colref()?;
            self.expect(&Tok::Eq)?;
            let right = self.parse_colref()?;
            join = Some(Join { table, left, right });
        }

        let where_ = if self.eat_kw("WHERE") {
            Some(self.parse_cond_or()?)
        } else {
            None
        };

        let mut group_by = Vec::new();
        if self.eat_kw("GROUP") {
            self.expect_kw("BY")?;
            loop {
                group_by.push(self.parse_colref()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }

        let mut order_by = Vec::new();
        if self.eat_kw("ORDER") {
            self.expect_kw("BY")?;
            loop {
                let by = if let Some(Tok::Num(number)) = self.peek() {
                    let ordinal = exact_usize(*number, "ORDER BY ordinal")?;
                    self.next();
                    OrderBy::Ordinal(ordinal)
                } else {
                    let column = self.parse_colref()?;
                    OrderBy::Name(column.name)
                };
                let desc = if self.eat_kw("DESC") {
                    true
                } else {
                    self.eat_kw("ASC");
                    false
                };
                order_by.push(OrderKey { by, desc });
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }

        let limit = if self.eat_kw("LIMIT") {
            match self.next() {
                Some(Tok::Num(number)) => Some(exact_usize(number, "LIMIT")?),
                other => bail!("expected a number after LIMIT, found {other:?}"),
            }
        } else {
            None
        };

        if let Some(t) = self.peek() {
            bail!("unexpected trailing token in query: {t:?}");
        }

        Ok(SelectStmt {
            items,
            from,
            join,
            where_,
            group_by,
            order_by,
            limit,
        })
    }

    fn parse_select_item(&mut self) -> Result<SelectItem> {
        if self.eat(&Tok::Star) {
            return Ok(SelectItem::Star);
        }
        // Aggregate? ident immediately followed by '('.
        let agg = match (self.peek(), self.peek2()) {
            (Some(Tok::Ident(name)), Some(Tok::LParen)) => agg_fn(name),
            _ => None,
        };
        if let Some(agg) = agg {
            self.next(); // ident
            self.next(); // (
            let aggregate_arg = if self.eat(&Tok::Star) {
                None
            } else {
                Some(self.parse_colref()?)
            };
            self.expect(&Tok::RParen)?;
            let alias = self.parse_optional_alias()?;
            return Ok(SelectItem::Expr {
                expr: SelExpr::Agg(agg, aggregate_arg),
                alias,
            });
        }
        let col = self.parse_colref()?;
        let alias = self.parse_optional_alias()?;
        Ok(SelectItem::Expr {
            expr: SelExpr::Col(col),
            alias,
        })
    }

    fn parse_optional_alias(&mut self) -> Result<Option<String>> {
        // `AS alias`, or a bare trailing identifier that isn't a clause keyword.
        if self.eat_kw("AS") || self.peek_plain_ident() {
            Ok(Some(self.expect_ident()?))
        } else {
            Ok(None)
        }
    }

    fn parse_table_ref(&mut self) -> Result<TableRef> {
        let name = self.expect_ident()?;
        let alias = if self.eat_kw("AS") || self.peek_plain_ident() {
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok(TableRef { name, alias })
    }

    fn parse_colref(&mut self) -> Result<ColRef> {
        let a = self.expect_ident()?;
        if self.eat(&Tok::Dot) {
            let b = self.expect_ident()?;
            Ok(ColRef {
                table: Some(a),
                name: b,
            })
        } else {
            Ok(ColRef {
                table: None,
                name: a,
            })
        }
    }

    fn parse_cond_or(&mut self) -> Result<Cond> {
        let mut left = self.parse_cond_and()?;
        while self.eat_kw("OR") {
            let right = self.parse_cond_and()?;
            left = Cond::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_cond_and(&mut self) -> Result<Cond> {
        let mut left = self.parse_cond_not()?;
        while self.eat_kw("AND") {
            let right = self.parse_cond_not()?;
            left = Cond::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_cond_not(&mut self) -> Result<Cond> {
        if self.eat_kw("NOT") {
            return Ok(Cond::Not(Box::new(self.parse_cond_not()?)));
        }
        if self.eat(&Tok::LParen) {
            let c = self.parse_cond_or()?;
            self.expect(&Tok::RParen)?;
            return Ok(c);
        }
        // comparison: operand (op operand | LIKE str)
        let left = self.parse_operand()?;
        if self.eat_kw("LIKE") {
            let pat = match self.next() {
                Some(Tok::Str(s)) => s,
                other => bail!("expected a string after LIKE, found {other:?}"),
            };
            return Ok(Cond::Like(left, pat));
        }
        let op = match self.next() {
            Some(Tok::Eq) => CmpOp::Eq,
            Some(Tok::Ne) => CmpOp::Ne,
            Some(Tok::Lt) => CmpOp::Lt,
            Some(Tok::Le) => CmpOp::Le,
            Some(Tok::Gt) => CmpOp::Gt,
            Some(Tok::Ge) => CmpOp::Ge,
            other => bail!("expected a comparison operator, found {other:?}"),
        };
        let right = self.parse_operand()?;
        Ok(Cond::Cmp(left, op, right))
    }
    fn parse_operand(&mut self) -> Result<Operand> {
        match self.peek() {
            Some(Tok::Num(n)) => {
                let n = *n;
                self.next();
                Ok(Operand::Num(n))
            }
            Some(Tok::Str(s)) => {
                let s = s.clone();
                self.next();
                Ok(Operand::Str(s))
            }
            Some(Tok::Ident(_)) => Ok(Operand::Col(self.parse_colref()?)),
            other => bail!("expected a value or column, found {other:?}"),
        }
    }
}

fn agg_fn(name: &str) -> Option<AggFn> {
    match name.to_ascii_uppercase().as_str() {
        "SUM" => Some(AggFn::Sum),
        "COUNT" => Some(AggFn::Count),
        "AVG" | "MEAN" | "AVERAGE" => Some(AggFn::Avg),
        "MIN" => Some(AggFn::Min),
        "MAX" => Some(AggFn::Max),
        _ => None,
    }
}

fn exact_usize(number: f64, label: &str) -> Result<usize> {
    if !number.is_finite() || number.is_sign_negative() || number.fract() != 0.0 {
        bail!("{label} must be a non-negative integer, found {number}");
    }
    number
        .to_string()
        .parse::<usize>()
        .map_err(|_| anyhow!("{label} is outside the supported range: {number}"))
}

// ─── Table model + execution ──────────────────────────────────────────────────

struct Table {
    alias: String,        // lower-cased name used to qualify columns
    headers: Vec<String>, // original-case header text (empty == none)
    ncols: usize,
    rows: Vec<Vec<CellValue>>,
}

impl Table {
    fn from_sheet(wb: &Workbook, idx: usize, alias: &str) -> Table {
        let (rows, cols) = wb.sheets[idx].dimensions();
        let headers = (0..cols).map(|c| wb.display_cell(idx, 0, c)).collect();
        let data = (1..rows)
            .map(|r| (0..cols).map(|c| wb.sheets[idx].value(r, c)).collect())
            .collect();
        Table {
            alias: alias.to_ascii_lowercase(),
            headers,
            ncols: cols as usize,
            rows: data,
        }
    }

    /// Resolve a column name (header, case-insensitive) or letter to an index.
    fn col_index(&self, name: &str) -> Option<usize> {
        if let Some(i) = self
            .headers
            .iter()
            .position(|h| !h.is_empty() && h.eq_ignore_ascii_case(name))
        {
            return Some(i);
        }
        if !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphabetic())
            && let Some(column) = col_letters_to_index(&name.to_ascii_uppercase())
            && let Ok(column) = usize::try_from(column)
            && column < self.ncols
        {
            return Some(column);
        }
        None
    }

    fn header_name(&self, ci: usize) -> String {
        let h = self.headers.get(ci).cloned().unwrap_or_default();
        if h.is_empty() {
            u32::try_from(ci).map_or_else(
                |_| format!("COL{}", ci.saturating_add(1)),
                easyexcel::model::addr::col_index_to_letters,
            )
        } else {
            h
        }
    }
}

/// A combined row = one source-row index per table (aligned with `tables`).
type CombinedRow = Vec<usize>;

fn execute(wb: &Workbook, stmt: &SelectStmt) -> Result<QueryResult> {
    // Resolve the FROM table (+ optional JOIN table) to `Table`s.
    let mut tables = Vec::new();
    let from_idx = find_sheet(wb, &stmt.from.name)?;
    let from_alias = stmt
        .from
        .alias
        .clone()
        .unwrap_or_else(|| stmt.from.name.clone());
    tables.push(Table::from_sheet(wb, from_idx, &from_alias));

    if let Some(j) = &stmt.join {
        let jidx = find_sheet(wb, &j.table.name)?;
        let jalias = j
            .table
            .alias
            .clone()
            .unwrap_or_else(|| j.table.name.clone());
        tables.push(Table::from_sheet(wb, jidx, &jalias));
    }

    // Build the set of combined rows (single table or hash equi-join).
    let combined = build_rows(&tables, stmt)?;

    // Apply WHERE.
    let filtered: Vec<CombinedRow> = if let Some(cond) = &stmt.where_ {
        combined
            .into_iter()
            .filter(|row| eval_cond(cond, &tables, row).unwrap_or(false))
            .collect()
    } else {
        combined
    };

    // Expand SELECT * into concrete columns.
    let items = expand_items(&stmt.items, &tables);
    let has_agg = items.iter().any(|it| {
        matches!(
            it,
            ResolvedItem {
                expr: RExpr::Agg(..),
                ..
            }
        )
    });

    let (columns, rows) = if !stmt.group_by.is_empty() || has_agg {
        project_grouped(&tables, &items, &stmt.group_by, &filtered)?
    } else {
        project_plain(&tables, &items, &filtered)?
    };

    let mut result = QueryResult { columns, rows };
    apply_order_and_limit(&mut result, stmt)?;
    Ok(result)
}

fn find_sheet(wb: &Workbook, name: &str) -> Result<usize> {
    wb.sheet_index(name).ok_or_else(|| {
        let names: Vec<&str> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
        anyhow!("no sheet named '{name}' (available: {})", names.join(", "))
    })
}

fn build_rows(tables: &[Table], stmt: &SelectStmt) -> Result<Vec<CombinedRow>> {
    if tables.len() == 1 {
        return Ok((0..tables[0].rows.len()).map(|i| vec![i]).collect());
    }
    // Equi-join: index the right table by its join-key value, then probe.
    let j = stmt.join.as_ref().expect("two tables imply a join");
    let (lt, lc) = resolve_col(tables, &j.left)?;
    let (rt, rc) = resolve_col(tables, &j.right)?;
    // Normalize so left refers to table 0, right to table 1.
    let (l_idx, l_ci, r_idx, r_ci) = if lt == 0 {
        (lt, lc, rt, rc)
    } else {
        (rt, rc, lt, lc)
    };
    if l_idx == r_idx {
        bail!("JOIN ... ON must reference both tables");
    }
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (ri, row) in tables[r_idx].rows.iter().enumerate() {
        index.entry(value_key(&row[r_ci])).or_default().push(ri);
    }
    let mut out = Vec::new();
    for (li, lrow) in tables[l_idx].rows.iter().enumerate() {
        if let Some(matches) = index.get(&value_key(&lrow[l_ci])) {
            for &ri in matches {
                // combined indexed by table position (0 = from, 1 = join)
                let mut cr = vec![0usize; tables.len()];
                cr[l_idx] = li;
                cr[r_idx] = ri;
                out.push(cr);
            }
        }
    }
    Ok(out)
}

/// Resolve a column reference to `(table_index, column_index)`.
fn resolve_col(tables: &[Table], cr: &ColRef) -> Result<(usize, usize)> {
    if let Some(tname) = &cr.table {
        let ti = tables
            .iter()
            .position(|t| t.alias.eq_ignore_ascii_case(tname))
            .ok_or_else(|| anyhow!("unknown table/alias '{tname}'"))?;
        let ci = tables[ti]
            .col_index(&cr.name)
            .ok_or_else(|| anyhow!("no column '{}' in '{tname}'", cr.name))?;
        Ok((ti, ci))
    } else {
        // search all tables; first match wins
        for (ti, t) in tables.iter().enumerate() {
            if let Some(ci) = t.col_index(&cr.name) {
                return Ok((ti, ci));
            }
        }
        bail!("unknown column '{}'", cr.name)
    }
}

fn cell_at(tables: &[Table], row: &CombinedRow, ti: usize, ci: usize) -> CellValue {
    tables[ti].rows[row[ti]][ci].clone()
}

// ── value helpers ──────────────────────────────────────────────────────────

/// A normalized string key for equality/grouping (numbers via General format).
fn value_key(v: &CellValue) -> String {
    match v {
        CellValue::Number(n) => format_number_general(*n),
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Text(s) => s.clone(),
        CellValue::Empty => String::new(),
        CellValue::Error(e) => e.as_str().to_string(),
    }
}

/// A numeric view of a value for comparisons/aggregates (text-numbers parsed).
fn as_num(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Number(n) => Some(*n),
        CellValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        CellValue::Text(s) => parse_number_text(s),
        _ => None,
    }
}

fn operand_value(op: &Operand, tables: &[Table], row: &CombinedRow) -> Result<CellValue> {
    Ok(match op {
        Operand::Num(n) => CellValue::Number(*n),
        Operand::Str(s) => CellValue::Text(s.clone()),
        Operand::Col(cr) => {
            let (ti, ci) = resolve_col(tables, cr)?;
            cell_at(tables, row, ti, ci)
        }
    })
}

fn eval_cond(cond: &Cond, tables: &[Table], row: &CombinedRow) -> Result<bool> {
    Ok(match cond {
        Cond::And(a, b) => eval_cond(a, tables, row)? && eval_cond(b, tables, row)?,
        Cond::Or(a, b) => eval_cond(a, tables, row)? || eval_cond(b, tables, row)?,
        Cond::Not(a) => !eval_cond(a, tables, row)?,
        Cond::Cmp(l, op, r) => {
            let lv = operand_value(l, tables, row)?;
            let rv = operand_value(r, tables, row)?;
            compare(&lv, *op, &rv)
        }
        Cond::Like(l, pat) => {
            let lv = operand_value(l, tables, row)?;
            like_match(&value_key(&lv), pat)
        }
    })
}

fn compare(l: &CellValue, op: CmpOp, r: &CellValue) -> bool {
    let ord = match (as_num(l), as_num(r)) {
        (Some(a), Some(b)) => a.partial_cmp(&b),
        _ => Some(value_key(l).cmp(&value_key(r))),
    };
    let Some(ord) = ord else { return false };
    match op {
        CmpOp::Eq => ord == Equal,
        CmpOp::Ne => ord != Equal,
        CmpOp::Lt => ord == Less,
        CmpOp::Le => ord != Greater,
        CmpOp::Gt => ord == Greater,
        CmpOp::Ge => ord != Less,
    }
}

/// SQL `LIKE` (case-insensitive here): `%` = any run, `_` = one char.
fn like_match(s: &str, pattern: &str) -> bool {
    let mut re = String::from("(?i)^");
    for ch in pattern.chars() {
        match ch {
            '%' => re.push_str(".*"),
            '_' => re.push('.'),
            c => {
                if "\\.^$|?*+()[]{}".contains(c) {
                    re.push('\\');
                }
                re.push(c);
            }
        }
    }
    re.push('$');
    regex::Regex::new(&re).is_ok_and(|regex| regex.is_match(s))
}

// ── projection ───────────────────────────────────────────────────────────────

struct ResolvedItem {
    expr: RExpr,
    name: String,
}
enum RExpr {
    Col(usize, usize), // (table, col)
    Agg(AggFn, Option<(usize, usize)>),
}

fn expand_items(items: &[SelectItem], tables: &[Table]) -> Vec<ResolvedItem> {
    let mut out = Vec::new();
    let multi = tables.len() > 1;
    for it in items {
        match it {
            SelectItem::Star => {
                for (ti, t) in tables.iter().enumerate() {
                    for ci in 0..t.ncols {
                        let base = t.header_name(ci);
                        let name = if multi {
                            format!("{}.{}", t.alias, base)
                        } else {
                            base
                        };
                        out.push(ResolvedItem {
                            expr: RExpr::Col(ti, ci),
                            name,
                        });
                    }
                }
            }
            SelectItem::Expr { expr, alias } => match expr {
                SelExpr::Col(cr) => {
                    let resolved = resolve_col(tables, cr);
                    let name = alias.clone().unwrap_or_else(|| cr.name.clone());
                    match resolved {
                        Ok((ti, ci)) => out.push(ResolvedItem {
                            expr: RExpr::Col(ti, ci),
                            name,
                        }),
                        // Defer the error to execution so messages stay consistent;
                        // resolve_col is re-run there and surfaces the failure.
                        Err(_) => out.push(ResolvedItem {
                            expr: RExpr::Col(usize::MAX, usize::MAX),
                            name,
                        }),
                    }
                }
                SelExpr::Agg(f, arg) => {
                    let resolved = arg.as_ref().and_then(|cr| resolve_col(tables, cr).ok());
                    let label = alias.clone().unwrap_or_else(|| {
                        let inner = arg
                            .as_ref()
                            .map_or_else(|| "*".to_string(), |column| column.name.clone());
                        format!("{}({})", agg_name(*f), inner)
                    });
                    out.push(ResolvedItem {
                        expr: RExpr::Agg(*f, resolved),
                        name: label,
                    });
                }
            },
        }
    }
    out
}

fn agg_name(f: AggFn) -> &'static str {
    match f {
        AggFn::Sum => "SUM",
        AggFn::Count => "COUNT",
        AggFn::Avg => "AVG",
        AggFn::Min => "MIN",
        AggFn::Max => "MAX",
    }
}

fn project_plain(
    tables: &[Table],
    items: &[ResolvedItem],
    rows: &[CombinedRow],
) -> Result<(Vec<String>, Vec<Vec<CellValue>>)> {
    let columns = items.iter().map(|it| it.name.clone()).collect();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut vals = Vec::with_capacity(items.len());
        for it in items {
            match &it.expr {
                RExpr::Col(ti, ci) => {
                    if *ti == usize::MAX {
                        bail!("unknown column '{}'", it.name);
                    }
                    vals.push(cell_at(tables, row, *ti, *ci));
                }
                RExpr::Agg(..) => bail!("aggregate '{}' requires GROUP BY", it.name),
            }
        }
        out.push(vals);
    }
    Ok((columns, out))
}

fn project_grouped(
    tables: &[Table],
    items: &[ResolvedItem],
    group_by: &[ColRef],
    rows: &[CombinedRow],
) -> Result<(Vec<String>, Vec<Vec<CellValue>>)> {
    let group_cols: Vec<(usize, usize)> = group_by
        .iter()
        .map(|cr| resolve_col(tables, cr))
        .collect::<Result<_>>()?;

    // Group rows by the tuple of group-column values, preserving first-seen order.
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let key = group_cols
            .iter()
            .map(|&(ti, ci)| value_key(&cell_at(tables, row, ti, ci)))
            .collect::<Vec<_>>()
            .join("\u{1}");
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(i);
    }
    // No GROUP BY but aggregates present → a single group over all rows.
    if group_by.is_empty() {
        order.clear();
        let all: Vec<usize> = (0..rows.len()).collect();
        order.push(String::new());
        groups.clear();
        groups.insert(String::new(), all);
    }

    let columns = items.iter().map(|it| it.name.clone()).collect();
    let mut out = Vec::with_capacity(order.len());
    for key in &order {
        let member_idx = &groups[key];
        let members: Vec<&CombinedRow> = member_idx.iter().map(|&i| &rows[i]).collect();
        let mut vals = Vec::with_capacity(items.len());
        for it in items {
            match &it.expr {
                RExpr::Col(ti, ci) => {
                    if *ti == usize::MAX {
                        bail!("unknown column '{}'", it.name);
                    }
                    // Non-aggregated column → first row of the group.
                    let first = members.first().expect("non-empty group");
                    vals.push(cell_at(tables, first, *ti, *ci));
                }
                RExpr::Agg(f, arg) => vals.push(aggregate(tables, *f, *arg, &members)),
            }
        }
        out.push(vals);
    }
    Ok((columns, out))
}

fn count_as_f64(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

fn aggregate(
    tables: &[Table],
    f: AggFn,
    arg: Option<(usize, usize)>,
    members: &[&CombinedRow],
) -> CellValue {
    match f {
        AggFn::Count => match arg {
            None => CellValue::Number(count_as_f64(members.len())),
            Some((ti, ci)) => {
                let n = members
                    .iter()
                    .filter(|row| !matches!(cell_at(tables, row, ti, ci), CellValue::Empty))
                    .count();
                CellValue::Number(count_as_f64(n))
            }
        },
        AggFn::Sum | AggFn::Avg => {
            let Some((ti, ci)) = arg else {
                return CellValue::Error(easyexcel::model::CellError::Value);
            };
            let nums: Vec<f64> = members
                .iter()
                .filter_map(|row| as_num(&cell_at(tables, row, ti, ci)))
                .collect();
            let sum: f64 = nums.iter().sum();
            match f {
                AggFn::Sum => CellValue::Number(sum),
                _ if nums.is_empty() => CellValue::Empty,
                _ => CellValue::Number(sum / count_as_f64(nums.len())),
            }
        }
        AggFn::Min | AggFn::Max => {
            let Some((ti, ci)) = arg else {
                return CellValue::Error(easyexcel::model::CellError::Value);
            };
            let vals: Vec<CellValue> = members
                .iter()
                .map(|row| cell_at(tables, row, ti, ci))
                .filter(|v| !matches!(v, CellValue::Empty))
                .collect();
            if vals.is_empty() {
                return CellValue::Empty;
            }
            // Numeric min/max when every value is numeric, else lexical.
            if vals.iter().all(|v| as_num(v).is_some()) {
                let nums = vals.iter().map(|v| as_num(v).unwrap());
                let best = match f {
                    AggFn::Min => nums.fold(f64::INFINITY, f64::min),
                    _ => nums.fold(f64::NEG_INFINITY, f64::max),
                };
                CellValue::Number(best)
            } else {
                let best = match f {
                    AggFn::Min => vals.iter().min_by(|a, b| value_key(a).cmp(&value_key(b))),
                    _ => vals.iter().max_by(|a, b| value_key(a).cmp(&value_key(b))),
                };
                best.cloned().unwrap_or(CellValue::Empty)
            }
        }
    }
}

fn apply_order_and_limit(result: &mut QueryResult, stmt: &SelectStmt) -> Result<()> {
    if !stmt.order_by.is_empty() {
        // Resolve each order key to an output column index.
        let keys: Vec<(usize, bool)> = stmt
            .order_by
            .iter()
            .map(|k| {
                let idx = match &k.by {
                    OrderBy::Ordinal(n) => {
                        if *n == 0 || *n > result.columns.len() {
                            bail!("ORDER BY ordinal {n} is out of range");
                        }
                        *n - 1
                    }
                    OrderBy::Name(name) => result
                        .columns
                        .iter()
                        .position(|c| c.eq_ignore_ascii_case(name))
                        .ok_or_else(|| {
                            anyhow!("ORDER BY '{name}' is not a selected column or alias")
                        })?,
                };
                Ok((idx, k.desc))
            })
            .collect::<Result<_>>()?;

        result.rows.sort_by(|a, b| {
            for &(ci, desc) in &keys {
                let av = &a[ci];
                let bv = &b[ci];
                let ord = match (as_num(av), as_num(bv)) {
                    (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    _ => value_key(av).cmp(&value_key(bv)),
                };
                let ord = if desc { ord.reverse() } else { ord };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    if let Some(n) = stmt.limit {
        result.rows.truncate(n);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyexcel::model::{Cell, Workbook};

    fn wb() -> Workbook {
        let mut wb = Workbook::new();
        wb.sheets[0].name = "txns".into();
        let s = wb.sheet_mut(0).unwrap();
        s.set_a1("A1", Cell::Text("desc".into()));
        s.set_a1("B1", Cell::Text("category".into()));
        s.set_a1("C1", Cell::Text("amount".into()));
        let rows = [
            ("PETROL", "fuel", 1200.0),
            ("WAZIRX", "crypto", 5000.0),
            ("PETROL2", "fuel", 800.0),
            ("WAZIRX2", "crypto", 2500.0),
            ("SALARY", "income", 75000.0),
        ];
        for (i, (d, c, a)) in rows.iter().enumerate() {
            let r = u32::try_from(i).expect("fixture row index fits u32") + 2;
            s.set_a1(&format!("A{r}"), Cell::Text((*d).into()));
            s.set_a1(&format!("B{r}"), Cell::Text((*c).into()));
            s.set_a1(&format!("C{r}"), Cell::Number(*a));
        }
        wb
    }

    fn q(wb: &Workbook, sql: &str) -> QueryResult {
        run_query(wb, sql).unwrap()
    }

    #[test]
    fn select_star() {
        let r = q(&wb(), "SELECT * FROM txns");
        assert_eq!(r.columns, vec!["desc", "category", "amount"]);
        assert_eq!(r.rows.len(), 5);
    }

    #[test]
    fn select_columns_and_where() {
        let r = q(&wb(), "SELECT desc, amount FROM txns WHERE amount > 1000");
        assert_eq!(r.columns, vec!["desc", "amount"]);
        // PETROL(1200), WAZIRX(5000), WAZIRX2(2500), SALARY(75000)
        assert_eq!(r.rows.len(), 4);
    }

    #[test]
    fn where_string_and_and_or() {
        let r = q(
            &wb(),
            "SELECT desc FROM txns WHERE category = 'fuel' AND amount < 1000",
        );
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0][0], CellValue::Text("PETROL2".into()));
    }

    #[test]
    fn like_match_query() {
        let r = q(&wb(), "SELECT desc FROM txns WHERE desc LIKE 'WAZIRX%'");
        assert_eq!(r.rows.len(), 2);
    }

    #[test]
    fn group_by_sum_order_limit() {
        let r = q(
            &wb(),
            "SELECT category, SUM(amount) AS total FROM txns GROUP BY category ORDER BY total DESC LIMIT 2",
        );
        assert_eq!(r.columns, vec!["category", "total"]);
        assert_eq!(r.rows.len(), 2);
        // income 75000 first, then crypto 7500
        assert_eq!(r.rows[0][0], CellValue::Text("income".into()));
        assert_eq!(r.rows[0][1], CellValue::Number(75000.0));
        assert_eq!(r.rows[1][0], CellValue::Text("crypto".into()));
        assert_eq!(r.rows[1][1], CellValue::Number(7500.0));
    }

    #[test]
    fn count_and_avg() {
        let r = q(
            &wb(),
            "SELECT category, COUNT(*) AS n, AVG(amount) AS a FROM txns GROUP BY category",
        );
        // 3 groups: fuel, crypto, income
        assert_eq!(r.rows.len(), 3);
        // find fuel row
        let fuel = r
            .rows
            .iter()
            .find(|row| row[0] == CellValue::Text("fuel".into()))
            .unwrap();
        assert_eq!(fuel[1], CellValue::Number(2.0));
        assert_eq!(fuel[2], CellValue::Number(1000.0)); // (1200+800)/2
    }

    #[test]
    fn cross_sheet_join() {
        let mut w = wb();
        let bi = w.add_sheet("budget");
        let s = w.sheet_mut(bi).unwrap();
        s.set_a1("A1", Cell::Text("category".into()));
        s.set_a1("B1", Cell::Text("budget".into()));
        s.set_a1("A2", Cell::Text("fuel".into()));
        s.set_a1("B2", Cell::Number(3000.0));
        s.set_a1("A3", Cell::Text("crypto".into()));
        s.set_a1("B3", Cell::Number(10000.0));
        let r = q(
            &w,
            "SELECT t.desc, t.amount, b.budget FROM txns t JOIN budget b ON t.category = b.category",
        );
        assert_eq!(r.columns, vec!["desc", "amount", "budget"]);
        // only fuel (2) + crypto (2) rows join; income has no budget
        assert_eq!(r.rows.len(), 4);
    }

    #[test]
    fn unknown_sheet_errors() {
        assert!(run_query(&wb(), "SELECT * FROM nope").is_err());
    }

    #[test]
    fn column_letters_work() {
        let r = q(&wb(), "SELECT A, C FROM txns WHERE C >= 5000");
        // WAZIRX(5000), SALARY(75000)
        assert_eq!(r.rows.len(), 2);
    }
}
