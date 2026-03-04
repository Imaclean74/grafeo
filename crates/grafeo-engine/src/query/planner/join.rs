//! Join, union, and distinct planning.

use super::common;
use super::*;

impl super::Planner {
    /// Plans a JOIN operator.
    pub(super) fn plan_join(&self, join: &JoinOp) -> Result<(Box<dyn Operator>, Vec<String>)> {
        let (left_op, left_columns) = self.plan_operator(&join.left)?;
        let (right_op, right_columns) = self.plan_operator(&join.right)?;

        // Build combined output columns
        let mut columns = left_columns.clone();
        columns.extend(right_columns.clone());

        // Convert join type
        let physical_join_type = match join.join_type {
            JoinType::Inner => PhysicalJoinType::Inner,
            JoinType::Left => PhysicalJoinType::Left,
            JoinType::Right => PhysicalJoinType::Right,
            JoinType::Full => PhysicalJoinType::Full,
            JoinType::Cross => PhysicalJoinType::Cross,
            JoinType::Semi => PhysicalJoinType::Semi,
            JoinType::Anti => PhysicalJoinType::Anti,
        };

        // Build key columns from join conditions
        let (probe_keys, build_keys): (Vec<usize>, Vec<usize>) = if join.conditions.is_empty() {
            // Cross join - no keys
            (vec![], vec![])
        } else {
            join.conditions
                .iter()
                .filter_map(|cond| {
                    // Try to extract column indices from expressions
                    let left_idx = self.expression_to_column(&cond.left, &left_columns).ok()?;
                    let right_idx = self
                        .expression_to_column(&cond.right, &right_columns)
                        .ok()?;
                    Some((left_idx, right_idx))
                })
                .unzip()
        };

        let output_schema = self.derive_schema_from_columns(&columns);

        // Check if we should use leapfrog join for cyclic patterns
        // Currently we use hash join by default; leapfrog is available but
        // requires explicit multi-way join detection which will be added
        // when we have proper cyclic pattern detection in the optimizer.
        // For now, LeapfrogJoinOperator is available for direct use.
        let _ = LeapfrogJoinOperator::new; // Suppress unused warning

        let operator: Box<dyn Operator> = Box::new(HashJoinOperator::new(
            left_op,
            right_op,
            probe_keys,
            build_keys,
            physical_join_type,
            output_schema,
        ));

        Ok((operator, columns))
    }

    /// Extracts a column index from an expression.
    pub(super) fn expression_to_column(
        &self,
        expr: &LogicalExpression,
        columns: &[String],
    ) -> Result<usize> {
        match expr {
            LogicalExpression::Variable(name) => columns
                .iter()
                .position(|c| c == name)
                .ok_or_else(|| Error::Internal(format!("Variable '{}' not found", name))),
            _ => Err(Error::Internal(
                "Only variables supported in join conditions".to_string(),
            )),
        }
    }

    /// Plans a UNION operator.
    pub(super) fn plan_union(&self, union: &UnionOp) -> Result<(Box<dyn Operator>, Vec<String>)> {
        let mut inputs = Vec::with_capacity(union.inputs.len());
        let mut columns = Vec::new();

        for (i, input) in union.inputs.iter().enumerate() {
            let (op, cols) = self.plan_operator(input)?;
            if i == 0 {
                columns = cols;
            }
            inputs.push(op);
        }

        let schema = self.derive_schema_from_columns(&columns);
        common::build_union(inputs, columns, schema)
    }

    /// Plans a DISTINCT operator.
    pub(super) fn plan_distinct(
        &self,
        distinct: &DistinctOp,
    ) -> Result<(Box<dyn Operator>, Vec<String>)> {
        let (input_op, columns) = self.plan_operator(&distinct.input)?;
        let schema = self.derive_schema_from_columns(&columns);
        Ok(common::build_distinct(
            input_op,
            columns,
            distinct.columns.as_deref(),
            schema,
        ))
    }

    /// Plans an EXCEPT operator.
    pub(super) fn plan_except(
        &self,
        except: &ExceptOp,
    ) -> Result<(Box<dyn Operator>, Vec<String>)> {
        let (left_op, columns) = self.plan_operator(&except.left)?;
        let (right_op, _) = self.plan_operator(&except.right)?;
        let schema = self.derive_schema_from_columns(&columns);
        Ok(common::build_except(
            left_op, right_op, columns, except.all, schema,
        ))
    }

    /// Plans an INTERSECT operator.
    pub(super) fn plan_intersect(
        &self,
        intersect: &IntersectOp,
    ) -> Result<(Box<dyn Operator>, Vec<String>)> {
        let (left_op, columns) = self.plan_operator(&intersect.left)?;
        let (right_op, _) = self.plan_operator(&intersect.right)?;
        let schema = self.derive_schema_from_columns(&columns);
        Ok(common::build_intersect(
            left_op,
            right_op,
            columns,
            intersect.all,
            schema,
        ))
    }

    /// Plans an OTHERWISE operator.
    pub(super) fn plan_otherwise(
        &self,
        otherwise: &OtherwiseOp,
    ) -> Result<(Box<dyn Operator>, Vec<String>)> {
        let (left_op, columns) = self.plan_operator(&otherwise.left)?;
        let (right_op, _) = self.plan_operator(&otherwise.right)?;
        Ok(common::build_otherwise(left_op, right_op, columns))
    }

    /// Plans an APPLY (lateral join) operator.
    pub(super) fn plan_apply(&self, apply: &ApplyOp) -> Result<(Box<dyn Operator>, Vec<String>)> {
        let (outer_op, columns) = self.plan_operator(&apply.input)?;
        let (inner_op, inner_columns) = self.plan_operator(&apply.subplan)?;
        Ok(common::build_apply(
            outer_op,
            inner_op,
            columns,
            inner_columns,
        ))
    }
}
