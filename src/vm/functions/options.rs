use vm::errors::{CheckErrors, RuntimeErrorType, ShortReturnType, InterpreterResult as Result,
                 check_argument_count, check_arguments_at_least};
use vm::types::{Value, ResponseData, OptionalData, TypeSignature};
use vm::contexts::{LocalContext, Environment};
use vm::{SymbolicExpression, ClarityName};
use vm;

fn inner_unwrap(to_unwrap: Value) -> Result<Option<Value>> {
    let result = match to_unwrap {
        Value::Optional(data) => {
            match data.data {
                Some(data) => Some(*data),
                None => None
            }
        },
        Value::Response(data) => {
            if data.committed {
                Some(*data.data)
            } else {
                None
            }
        },
        _ => return Err(CheckErrors::ExpectedOptionalOrResponseValue(to_unwrap).into())
    };

    Ok(result)
}

fn inner_unwrap_err(to_unwrap: Value) -> Result<Option<Value>> {
    let result = match to_unwrap {
        Value::Response(data) => {
            if !data.committed {
                Some(*data.data)
            } else {
                None
            }
        },
        _ => return Err(CheckErrors::ExpectedResponseValue(to_unwrap).into())
    };

    Ok(result)
}

pub fn native_unwrap(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(1, &args)?;
    let input = args.pop().unwrap();

    inner_unwrap(input)
        .and_then(|opt_value| {
            match opt_value {
                Some(v) => Ok(v),
                None => Err(RuntimeErrorType::UnwrapFailure.into())
            }
        })
}

pub fn native_unwrap_or_ret(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(2, &args)?;

    args.reverse();

    let input = args.pop().unwrap();
    let thrown = args.pop().unwrap();

    inner_unwrap(input)
        .and_then(|opt_value| {
            match opt_value {
                Some(v) => Ok(v),
                None => Err(ShortReturnType::ExpectedValue(thrown).into())
            }
        })
}

pub fn native_unwrap_err(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(1, &args)?;
    let input = args.pop().unwrap();

    inner_unwrap_err(input)
        .and_then(|opt_value| {
            match opt_value {
                Some(v) => Ok(v),
                None => Err(RuntimeErrorType::UnwrapFailure.into())
            }
        })
}

pub fn native_unwrap_err_or_ret(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(2, &args)?;

    args.reverse();

    let input = args.pop().unwrap();
    let thrown = args.pop().unwrap();

    inner_unwrap_err(input)
        .and_then(|opt_value| {
            match opt_value {
                Some(v) => Ok(v),
                None => Err(ShortReturnType::ExpectedValue(thrown).into())
            }
        })
}

pub fn native_try_ret(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(1, &args)?;

    let input = args.pop().unwrap();

    match input {
        Value::Optional(data) => {
            match data.data {
                Some(data) => Ok(*data),
                None => Err(ShortReturnType::ExpectedValue(Value::none()).into())
            }
        },
        Value::Response(data) => {
            if data.committed {
                Ok(*data.data)
            } else {
                Err(ShortReturnType::ExpectedValue(*data.data).into())
            }
        },
        _ => Err(CheckErrors::ExpectedOptionalOrResponseValue(input).into())
    }
}

fn eval_with_new_binding(body: &SymbolicExpression, bind_name: ClarityName, bind_value: Value, 
                         env: &mut Environment, context: &LocalContext) -> Result<Value> {
    let mut inner_context = context.extend()?;
    if vm::is_reserved(&bind_name) ||
       env.contract_context.lookup_function(&bind_name).is_some() ||
       inner_context.lookup_variable(&bind_name).is_some() {
        return Err(CheckErrors::NameAlreadyUsed(bind_name.into()).into())
    }

    inner_context.variables.insert(bind_name, bind_value);

    vm::eval(body, env, &inner_context)
}

fn special_match_opt(input: OptionalData, args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    if args.len() != 3 {
        Err(CheckErrors::BadMatchOptionSyntax(
            Box::new(CheckErrors::IncorrectArgumentCount(4, args.len()+1))))?;
    }

    let bind_name = args[0].match_atom()
        .ok_or_else(
            || CheckErrors::BadMatchOptionSyntax(Box::new(CheckErrors::ExpectedName)))?
        .clone();
    let some_branch = &args[1];
    let none_branch = &args[2];

    match input.data {
        Some(data) => eval_with_new_binding(some_branch, bind_name, *data, env, context),
        None => vm::eval(none_branch, env, context)
    }
}


fn special_match_resp(input: ResponseData, args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    if args.len() != 4 {
        Err(CheckErrors::BadMatchResponseSyntax(
            Box::new(CheckErrors::IncorrectArgumentCount(5, args.len()+1))))?;
    }

    let ok_bind_name = args[0].match_atom()
        .ok_or_else(
            || CheckErrors::BadMatchResponseSyntax(Box::new(CheckErrors::ExpectedName)))?
        .clone();
    let ok_branch = &args[1];
    let err_bind_name = args[2].match_atom()
        .ok_or_else(
            || CheckErrors::BadMatchResponseSyntax(Box::new(CheckErrors::ExpectedName)))?
        .clone();
    let err_branch = &args[3];

    if input.committed {
        eval_with_new_binding(ok_branch, ok_bind_name, *input.data, env, context)
    } else {
        eval_with_new_binding(err_branch, err_bind_name, *input.data, env, context)
    }
}

pub fn special_match(args: &[SymbolicExpression], env: &mut Environment, context: &LocalContext) -> Result<Value> {
    check_arguments_at_least(1, args)?;

    let input = vm::eval(&args[0], env, context)?;

    match input {
        Value::Response(data) => {
            special_match_resp(data, &args[1..], env, context) 
        },
        Value::Optional(data) => {
            special_match_opt(data, &args[1..], env, context) 
        },
        _ => return Err(CheckErrors::BadMatchInput(TypeSignature::type_of(&input)).into())
    }
}

pub fn native_some(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(1, &args)?;

    Ok(Value::some(args.pop().unwrap()))
}

fn is_some(mut args: Vec<Value>) -> Result<bool> {
    check_argument_count(1, &args)?;
    let input = args.pop().unwrap();

    match input {
        Value::Optional(ref data) => Ok(data.data.is_some()),
        _ => Err(CheckErrors::ExpectedOptionalValue(input).into())
    }
}

fn is_okay(mut args: Vec<Value>) -> Result<bool> {
    check_argument_count(1, &args)?;
    let input = args.pop().unwrap();

    match input {
        Value::Response(data) => Ok(data.committed),
        _ => Err(CheckErrors::ExpectedResponseValue(input).into())
    }
}

pub fn native_is_some(args: Vec<Value>) -> Result<Value> {
    is_some(args)
        .map(|is_some| { Value::Bool(is_some) })
}

pub fn native_is_none(args: Vec<Value>) -> Result<Value> {
    is_some(args)
        .map(|is_some| { Value::Bool(!is_some) })
}

pub fn native_is_okay(args: Vec<Value>) -> Result<Value> {
    is_okay(args)
        .map(|is_ok| { Value::Bool(is_ok) })
}

pub fn native_is_err(args: Vec<Value>) -> Result<Value> {
    is_okay(args)
        .map(|is_ok| { Value::Bool(!is_ok) })
}

pub fn native_okay(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(1, &args)?;
    let input = args.pop().unwrap();

    Ok(Value::okay(input))
}

pub fn native_error(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(1, &args)?;
    let input = args.pop().unwrap();
        
    Ok(Value::error(input))
}

pub fn native_default_to(mut args: Vec<Value>) -> Result<Value> {
    check_argument_count(2, &args)?;
    args.reverse();

    let default = args.pop().unwrap();
    let input = args.pop().unwrap();

    match input {
        Value::Optional(data) => {
            match data.data {
                Some(data) => Ok(*data),
                None => Ok(default)
            }
        },
        _ => Err(CheckErrors::ExpectedOptionalValue(input).into())
    }
}
