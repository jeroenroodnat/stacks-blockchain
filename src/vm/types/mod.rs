pub mod serialization;
mod signatures;

use std::hash::{Hash, Hasher};
use std::{fmt, cmp};
use std::collections::BTreeMap;

use address::c32;
use vm::representations::{SymbolicExpression, SymbolicExpressionType};
use vm::errors::{RuntimeErrorType, UncheckedError, InterpreterResult as Result, IncomparableError};
use util::hash;

pub use vm::types::signatures::{
    AtomTypeIdentifier, TupleTypeSignature, AssetIdentifier,
    TypeSignature, FunctionType, ListTypeData, FunctionArg, parse_name_type_pairs};

pub const MAX_VALUE_SIZE: i128 = 1024 * 1024; // 1MB

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct TupleData {
    type_signature: TupleTypeSignature,
    pub data_map: BTreeMap<String, Value>
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuffData {
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct ListData {
    pub data: Vec<Value>,
    type_signature: TypeSignature
}

// a standard principal is a version byte + hash160 (20 bytes)
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct StandardPrincipalData(pub u8, pub [u8; 20]);

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum PrincipalData {
    StandardPrincipal(StandardPrincipalData),
    ContractPrincipal(String),
    QualifiedContractPrincipal { sender: StandardPrincipalData,
                                 name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionalData {
    pub data: Option<Box<Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseData {
    pub committed: bool,
    pub data: Box<Value>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum Value {
    Int(i128),
    Bool(bool),
    Buffer(BuffData),
    List(ListData),
    Principal(PrincipalData),
    Tuple(TupleData),
    Optional(OptionalData),
    Response(ResponseData)
}

#[derive(Debug)]
pub enum BlockInfoProperty {
    Time,
    VrfSeed,
    HeaderHash,
    BurnchainHeaderHash,
}

impl OptionalData {
    pub fn type_signature(&self) -> AtomTypeIdentifier {
        match self.data {
            Some(ref v) => AtomTypeIdentifier::OptionalType(Box::new(
                TypeSignature::type_of(&v))),
            None => AtomTypeIdentifier::OptionalType(Box::new(
                TypeSignature::new_atom(AtomTypeIdentifier::NoType)))
        }
    }
}

impl ResponseData {
    pub fn type_signature(&self) -> AtomTypeIdentifier {
        match self.committed {
            true => AtomTypeIdentifier::ResponseType(Box::new(
                (TypeSignature::type_of(&self.data), TypeSignature::new_atom(AtomTypeIdentifier::NoType)))),
            false => AtomTypeIdentifier::ResponseType(Box::new(
                (TypeSignature::new_atom(AtomTypeIdentifier::NoType), TypeSignature::type_of(&self.data))))
        }
    }
}

impl BlockInfoProperty {
    pub fn from_str(s: &str) -> Option<BlockInfoProperty> {
        use self::BlockInfoProperty::*;
        match s {
            "time" => Some(Time),
            "vrf-seed" => Some(VrfSeed),
            "header-hash" => Some(HeaderHash),
            "burnchain-header-hash" => Some(BurnchainHeaderHash),
            _ => None
        }
    }

    pub fn to_str(&self) -> &'static str {
        use self::BlockInfoProperty::*;
        match self {
            Time => "time",
            VrfSeed => "vrf-seed",
            HeaderHash => "header-hash",
            BurnchainHeaderHash => "burnchain-header-hash",
        }
    }

    pub fn type_result(&self) -> TypeSignature {
        use self::AtomTypeIdentifier::*;
        use self::BlockInfoProperty::*;
        match self {
            Time => TypeSignature::new_atom(IntType),
            VrfSeed => TypeSignature::new_atom(BufferType(32)),
            HeaderHash => TypeSignature::new_atom(BufferType(32)),
            BurnchainHeaderHash => TypeSignature::new_atom(BufferType(32)),
        }
    }
}

impl fmt::Display for BlockInfoProperty {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.to_str())
    }
}

impl PartialEq for ListData {
    fn eq(&self, other: &ListData) -> bool {
        self.data == other.data
    }
}

impl Hash for ListData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
}

impl Hash for OptionalData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
}

impl Hash for ResponseData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
}

impl PartialEq for TupleData {
    fn eq(&self, other: &TupleData) -> bool {
        self.data_map == other.data_map
    }
}

impl Hash for TupleData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data_map.hash(state);
    }
}

pub const NONE: Value = Value::Optional(OptionalData { data: None });

impl Value {
    pub fn some(data: Value) -> Value {
        Value::Optional(OptionalData {
            data: Some(Box::new(data)) })
    }

    pub fn none() -> Value {
        Value::Optional(OptionalData {
            data: None })
    }

    pub fn okay(data: Value) -> Value {
        Value::Response(ResponseData { 
            committed: true,
            data: Box::new(data) })
    }

    pub fn error(data: Value) -> Value {
        Value::Response(ResponseData { 
            committed: false,
            data: Box::new(data) })
    }

    pub fn static_none() -> &'static Value {
        &NONE
    }

    pub fn new_list(list_data: &[Value]) -> Result<Value> {
        let vec_data = Vec::from(list_data);
        Value::list_from(vec_data)
    }

    pub fn list_from(list_data: Vec<Value>) -> Result<Value> {
        let type_sig = TypeSignature::construct_parent_list_type(&list_data)?;
        // Aaron: at this point, we've _already_ allocated memory for this type.
        //     (e.g., from a (map...) call, or a (list...) call.
        //     this is a problem _if_ the static analyzer cannot already prevent
        //     this case. This applies to all the constructor size checks.
        if type_sig.size()? > MAX_VALUE_SIZE {
            return Err(RuntimeErrorType::ValueTooLarge.into())
        }
        Ok(Value::List(ListData { data: list_data, type_signature: type_sig }))
    }

    pub fn buff_from(buff_data: Vec<u8>) -> Result<Value> {
        if buff_data.len() > u32::max_value() as usize {
            Err(RuntimeErrorType::BufferTooLarge.into())
        } else if buff_data.len() as i128 > MAX_VALUE_SIZE {
            Err(RuntimeErrorType::ValueTooLarge.into())
        } else {
            Ok(Value::Buffer(BuffData { data: buff_data }))
        }
    }

    pub fn tuple_from_data(paired_tuple_data: Vec<(String, Value)>) -> Result<Value> {
        let tuple_data = TupleData::from_data(paired_tuple_data)?;
        if tuple_data.size()? > MAX_VALUE_SIZE {
            return Err(RuntimeErrorType::ValueTooLarge.into())
        }
        Ok(Value::Tuple(tuple_data))
    }

    pub fn size(&self) -> Result<i128> {
        match self {
            Value::Int(_i) => AtomTypeIdentifier::IntType.size(),
            Value::Bool(_i) => AtomTypeIdentifier::BoolType.size(),
            Value::Principal(_) => AtomTypeIdentifier::PrincipalType.size(),
            Value::Buffer(ref buff_data) => Ok(buff_data.data.len() as i128),
            Value::Tuple(ref tuple_data) => tuple_data.size(),
            Value::List(ref list_data) => list_data.type_signature.size(),
            Value::Optional(ref opt_data) => opt_data.type_signature().size(),
            Value::Response(ref res_data) => res_data.type_signature().size()
        }
    }

}

impl fmt::Display for OptionalData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.data {
            Some(ref x) => write!(f, "(some {})", x),
            None => write!(f, "none")
        }
    }
}

impl fmt::Display for ResponseData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.committed {
            true => write!(f, "(ok {})", self.data),
            false => write!(f, "(err {})", self.data)
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(int) => write!(f, "{}", int),
            Value::Bool(boolean) => write!(f, "{}", boolean),
            Value::Buffer(vec_bytes) => write!(f, "0x{}", hash::to_hex(&vec_bytes.data)),
            Value::Tuple(data) => write!(f, "{}", data),
            Value::Principal(principal_data) => write!(f, "{}", principal_data),
            Value::Optional(opt_data) => write!(f, "{}", opt_data),
            Value::Response(res_data) => write!(f, "{}", res_data),
            Value::List(list_data) => {
                write!(f, "( ")?;
                for v in list_data.data.iter() {
                    write!(f, "{} ", v)?;
                }
                write!(f, ")")
            }
        }
    }
}

impl PrincipalData {
    pub fn parse_qualified_contract_principal(literal: &str) -> Result<PrincipalData> {
        let split: Vec<_> = literal.splitn(2, ".").collect();
        if split.len() != 2 {
            return Err(RuntimeErrorType::ParseError(
                "Invalid principal literal: expected a `.` in a qualified contract name".to_string()).into());
        }
        let sender = Self::parse_standard_principal(split[0])?;
        let name = split[1].to_string();
        
        Ok(PrincipalData::QualifiedContractPrincipal { sender, name })
    }

    pub fn parse_standard_principal(literal: &str) -> Result<StandardPrincipalData> {
        let (version, data) = c32::c32_address_decode(&literal)
            .map_err(|x| { RuntimeErrorType::ParseError(format!("Invalid principal literal: {}", x)) })?;
        if data.len() != 20 {
            return Err(RuntimeErrorType::ParseError(
                "Invalid principal literal: Expected 20 data bytes.".to_string()).into());
        }
        let mut fixed_data = [0; 20];
        fixed_data.copy_from_slice(&data[..20]);
        Ok(StandardPrincipalData(version, fixed_data))
    }

    pub fn deserialize(json: &str) -> PrincipalData {
        serde_json::from_str(json)
            .expect("Failed to deserialize vm.PrincipalData")
    }
    pub fn serialize(&self) -> String {
        serde_json::to_string(self)
            .expect("Failed to serialize vm.PrincipalData")
    }
}

impl StandardPrincipalData {
    pub fn to_address(&self) -> String {
        c32::c32_address(self.0, &self.1[..])
            .unwrap_or_else(|_| "INVALID_C32_ADD".to_string())
    }
}

impl fmt::Display for PrincipalData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PrincipalData::StandardPrincipal(sender) => {
                let c32_str = sender.to_address();
                write!(f, "'{}", c32_str)                
            },
            PrincipalData::ContractPrincipal(contract_name) => {
                write!(f, "'CT{}", contract_name)
            },
            PrincipalData::QualifiedContractPrincipal { sender, name } => {
                let c32_str = sender.to_address();
                write!(f, "'CT{}.{}", c32_str, name)
            }
        }
    }
}

impl From<Vec<u8>> for Value {
    fn from(bytes: Vec<u8>) -> Self {
        Value::Buffer(BuffData { data: bytes })
    }
}

impl From<StandardPrincipalData> for Value {
    fn from(principal: StandardPrincipalData) -> Self {
        Value::Principal(PrincipalData::StandardPrincipal(principal))
    }
}

impl From<PrincipalData> for Value {
    fn from(p: PrincipalData) -> Self {
        Value::Principal(p)
    }
}

impl From<StandardPrincipalData> for PrincipalData {
    fn from(p: StandardPrincipalData) -> Self {
        PrincipalData::StandardPrincipal(p)
    }
}

impl TupleData {
    pub fn from_data(mut data: Vec<(String, Value)>) -> Result<TupleData> {
        let mut type_map = BTreeMap::new();
        let mut data_map = BTreeMap::new();
        for (name, value) in data.drain(..) {
            let type_info = TypeSignature::type_of(&value);
            if type_map.contains_key(&name) {
                return Err(UncheckedError::VariableDefinedMultipleTimes(name).into());
            } else {
                type_map.insert(name.clone(), type_info);
            }
            data_map.insert(name, value);
        }
        Ok(TupleData { type_signature: TupleTypeSignature { type_map: type_map },
                       data_map: data_map })

    }

    pub fn get(&self, name: &str) -> Result<Value> {
        if let Some(value) = self.data_map.get(name) {
            Ok(value.clone())
        } else {
            Err(UncheckedError::NoSuchTupleField.into())
        }
        
    }

    pub fn size(&self) -> Result<i128> {
        self.type_signature.size()
    }
}

impl fmt::Display for TupleData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut first = true;
        write!(f, "(tuple ")?;
        for (name, value) in self.data_map.iter() {
            if !first {
                write!(f, " ")?;
            }
            first = false;
            write!(f, "({} {})", name, value)?;
        }
        write!(f, ")")
    }
}
