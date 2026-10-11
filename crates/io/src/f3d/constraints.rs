//! Point, line and driving-dimension relationships observed in controlled sketches.
//! Other relationships sharing the generic constraint class fail explicitly.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

#[derive(Debug, PartialEq)]
pub struct SketchConstraints {
    pub geometry: u64,
    pub feature: u64,
    pub constraints: Vec<SketchConstraint>,
}

#[derive(Debug, PartialEq)]
pub enum SketchConstraint {
    Coincident { id: u64, points: [u64; 2] },
    Horizontal { id: u64, line: u64 },
    Vertical { id: u64, line: u64 },
    Linear { id: u64, points: [u64; 2], vertical: bool, binding: u64 },
    Diameter { id: u64, circle: u64, binding: u64 },
}

fn attributes(r: &mut Reader<'_>) -> Result<()> {
    for _ in 0..r.count(16)? {
        r.string()?;
        if r.string()? != "IntrinsicMetaTypeuint64" {
            return Err(invalid("unsupported constraint attribute type"));
        }
        r.u64()?;
    }
    Ok(())
}
fn one(r: &mut Reader<'_>) -> Result<()> {
    if r.take(1)? != [1] {
        return Err(invalid("unsupported constraint flags"));
    }
    Ok(())
}

impl Design {
    pub fn sketch_constraints(&self) -> Result<Vec<SketchConstraints>> {
        if self.layout != 8 {
            return Err(invalid("constraint decoding requires metadata layout 8"));
        }
        let Some(class) = self.classes.iter().find(|c| c.id == "44A64366-4BD3-4B24-881A-F94C206E8F2D") else { return Ok(vec![]) };
        if class.version != 18 {
            return Err(invalid("unsupported sketch geometry version"));
        }
        let records = Records::new(self);
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let parameters = self.parameters()?;
        let bindings: BTreeMap<_, _> = parameters.iter().map(|p| (p.binding, p.owner)).collect();
        let circles: BTreeMap<_, _> = self.sketch_circles()?.into_iter().map(|c| (c.id, c.sketch)).collect();
        let lines: BTreeMap<_, _> = self.sketch_lines()?.into_iter().map(|c| (c.id, c.sketch)).collect();
        class
            .objects
            .iter()
            .map(|geometry| {
                let mut r = payload(records.primary(*geometry)?, *geometry)?;
                empty(&mut r)?;
                if r.take(2)? != [0, 1] {
                    return Err(invalid("unsupported sketch attributes"));
                }
                attributes(&mut r)?;
                r.wide_string()?;
                r.u64()?;
                one(&mut r)?;
                let relations = references(&mut r)?;
                one(&mut r)?;
                let dimensions = references(&mut r)?;
                r.take(17)?;
                let feature = r.reference()?;
                require_class(&classes, feature, "8DA771B7-52ED-42FC-94E2-2353FE141373", 5)?;
                r.take(4)?;
                finish(&r)?;
                let mut seen = BTreeSet::new();
                let mut constraints = Vec::new();
                for id in relations {
                    if !seen.insert(id) {
                        return Err(invalid("duplicate sketch constraint"));
                    }
                    require_class(&classes, id, "60403D47-0C49-49B0-BDE8-1679608164A2", 3)?;
                    let mut r = payload(records.primary(id)?, id)?;
                    empty(&mut r)?;
                    one(&mut r)?;
                    let count = r.u32()?;
                    if count != 1 && count != 2 {
                        return Err(invalid("unsupported geometric constraint arity"));
                    }
                    let mut operands = Vec::new();
                    for _ in 0..count {
                        operands.push(r.reference()?);
                        zeros(&mut r, 4)?;
                    }
                    one(&mut r)?;
                    attributes(&mut r)?;
                    if r.reference()? != *geometry {
                        return Err(invalid("inconsistent coincidence owner"));
                    }
                    // The class is shared by geometric relationships; operand
                    // count and point types alone do not establish coincidence.
                    let kind = r.u64()?;
                    let back = references(&mut r)?;
                    let operands_set: BTreeSet<_> = operands.iter().copied().collect();
                    if back.len() != operands.len()
                        || operands_set.len() != operands.len()
                        || back.iter().copied().collect::<BTreeSet<_>>() != operands_set
                    {
                        return Err(invalid("inconsistent coincidence references"));
                    }
                    zeros(&mut r, 1)?;
                    finish(&r)?;
                    constraints.push(match (kind, operands.as_slice()) {
                        (1, [a, b]) => {
                            for p in [a, b] {
                                require_class(&classes, *p, "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11)?;
                            }
                            SketchConstraint::Coincident { id, points: [*a, *b] }
                        }
                        (64 | 128, [line]) if lines.get(line) == Some(geometry) => {
                            if kind == 64 {
                                SketchConstraint::Horizontal { id, line: *line }
                            } else {
                                SketchConstraint::Vertical { id, line: *line }
                            }
                        }
                        _ => return Err(invalid("unsupported geometric constraint kind")),
                    });
                }
                for id in dimensions {
                    if !seen.insert(id) {
                        return Err(invalid("duplicate sketch dimension"));
                    }
                    if classes.get(&id).is_some_and(|c| c.id == "8C780195-72C0-4a56-A911-E43AB14357F2") {
                        require_class(&classes, id, "8C780195-72C0-4a56-A911-E43AB14357F2", 3)?;
                        let mut r = payload(records.primary(id)?, id)?;
                        empty(&mut r)?;
                        one(&mut r)?;
                        if r.u32()? != 3 || r.reference()? != 0 {
                            return Err(invalid("unsupported linear dimension operands"));
                        }
                        let vertical = match r.u32()? {
                            4 => false,
                            5 => true,
                            _ => return Err(invalid("unsupported linear dimension orientation")),
                        };
                        let a = r.reference()?;
                        if r.u32()? != u32::from(vertical) {
                            return Err(invalid("unsupported linear dimension axis"));
                        }
                        let b = r.reference()?;
                        zeros(&mut r, 4)?;
                        if a == b {
                            return Err(invalid("duplicate dimension points"));
                        }
                        for p in [a, b] {
                            require_class(&classes, p, "C2CEDAE7-1716-47C1-B7B1-07B70081D0FB", 11)?;
                        }
                        one(&mut r)?;
                        attributes(&mut r)?;
                        r.take(224)?;
                        r.wide_string()?;
                        r.take(44)?;
                        if r.u64()? != if vertical { 0x800000 } else { 0x400000 } {
                            return Err(invalid("unsupported linear dimension flags"));
                        }
                        let binding = r.reference()?;
                        if bindings.get(&binding) != Some(&feature) || references(&mut r)? != [a, b] {
                            return Err(invalid("inconsistent linear dimension binding or points"));
                        }
                        zeros(&mut r, 1)?;
                        finish(&r)?;
                        dimension_owner(&records, id, *geometry)?;
                        constraints.push(SketchConstraint::Linear { id, points: [a, b], vertical, binding });
                        continue;
                    }
                    require_class(&classes, id, "664AD49F-207F-4CD5-8555-DD269E9F09EA", 0)?;
                    let mut r = payload(records.primary(id)?, id)?;
                    empty(&mut r)?;
                    one(&mut r)?;
                    if r.u32()? != 2 || r.reference()? != 0 || !matches!(r.u32()?, 4 | 5) {
                        return Err(invalid("unsupported diameter operands"));
                    }
                    let circle = r.reference()?;
                    zeros(&mut r, 4)?;
                    require_class(&classes, circle, "F0130424-8B7E-4092-93C9-1CA807482534", 0)?;
                    if circles.get(&circle) != Some(geometry) {
                        return Err(invalid("diameter circle belongs to another sketch"));
                    }
                    one(&mut r)?;
                    attributes(&mut r)?;
                    r.take(250)?; // presentation geometry, text prefix and layout
                    r.wide_string()?; // displayed value; the binding carries the expression
                    r.take(32)?;
                    let binding = r.reference()?;
                    if bindings.get(&binding) != Some(&feature) || references(&mut r)? != [circle] {
                        return Err(invalid("inconsistent diameter binding or curve"));
                    }
                    zeros(&mut r, 1)?;
                    finish(&r)?;
                    dimension_owner(&records, id, *geometry)?;
                    constraints.push(SketchConstraint::Diameter { id, circle, binding });
                }
                Ok(SketchConstraints { geometry: *geometry, feature, constraints })
            })
            .collect()
    }
}

fn dimension_owner(records: &Records<'_>, id: u64, geometry: u64) -> Result<()> {
    let mut r = payload(records.secondary(id)?, id)?;
    empty(&mut r)?;
    if r.reference()? != geometry {
        return Err(invalid("inconsistent dimension owner"));
    }
    r.take(18)?;
    if r.reference()? != geometry {
        return Err(invalid("inconsistent dimension owner back-reference"));
    }
    finish(&r)
}
