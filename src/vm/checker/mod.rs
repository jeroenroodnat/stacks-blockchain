mod typecheck;
mod errors;
mod identity_pass;
mod check_db;

use vm::representations::{SymbolicExpression};

pub use self::errors::{CheckResult, CheckError, CheckErrors};
pub use self::check_db::{AnalysisDatabase};

pub fn type_check(contract_name: &str, contract: &mut [SymbolicExpression], analysis_db: &mut AnalysisDatabase) -> CheckResult<()> {
    identity_pass::identity_pass(contract)?;
    let contract_analysis = typecheck::type_check_contract(contract, analysis_db)?;
    analysis_db.insert_contract(contract_name, &contract_analysis)?;
    Ok(())
}