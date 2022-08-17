use super::name::Name;
use super::types::SysDCType;
use super::token::{ TokenKind, Tokenizer };
use super::structure::{ SysDCSystem, SysDCUnit, SysDCData, SysDCModule, SysDCFunction, SysDCAnnotation, SysDCSpawn, SysDCSpawnChild };

// 複数要素を一気にパースするためのマクロ
// - 返り値: Vec<T>
// - 第一引数: Option<T>を返す関数呼び出し
// - 第二引数: TokenKindで表されるデリミタ(省略可)
macro_rules! parse_list {
    ($self:ident$(.$generator:ident)*($args:expr)) => {{
        let mut var_list = vec!();
        while let Some(elem) = $self$(.$generator)*($args) {
            var_list.push(elem);
        }
        var_list
    }};

    ($self:ident$(.$generator:ident)*($args:expr), $delimiter:expr) => {{
        let mut var_list = vec!();
        while let Some(elem) = $self$(.$generator)*($args) {
            var_list.push(elem);
            if $self.tokenizer.expect($delimiter).is_none() {
                break;
            }
        }
        var_list
    }};
}

pub struct Parser<'a> {
    tokenizer: Tokenizer<'a>
}

impl<'a> Parser<'a> {
    pub fn new(tokenizer: Tokenizer<'a>) -> Parser<'a> {
        Parser { tokenizer }
    }

    /**
     * <root> ::= { <sentence> }
     * <sentence> ::= { <data> | <module> }
     */
    pub fn parse(&mut self, namespace: &Name) -> SysDCUnit {
        let mut data = vec!();
        let mut modules = vec!();
        while self.tokenizer.has_token() {
            match (self.parse_data(namespace), self.parse_module(namespace)) {
                (None, None) => panic!("[ERROR] Data/Module not found, but tokens remain"),
                (d, m) => {
                    if d.is_some() { data.push(d.unwrap()); }
                    if m.is_some() { modules.push(m.unwrap()); }
                }
            }
        }
        SysDCUnit::new(namespace.clone(), data, modules)
    }

    /**
     * <data> ::= data <id> \{ <id_type_mapping_var_list, delimiter=,> \}
     */
    fn parse_data(&mut self, namespace: &Name) -> Option<SysDCData> {
        // data
        self.tokenizer.expect(TokenKind::Data)?;

        // <id>
        let name = Name::new(namespace, self.tokenizer.request(TokenKind::Identifier).get_id());

        // \{ <id_type_mapping_var_list, delimiter=,> \}
        self.tokenizer.request(TokenKind::BracketBegin);
        let member = parse_list!(self.parse_id_type_mapping_var(&name), TokenKind::Separater);
        self.tokenizer.request(TokenKind::BracketEnd);

        Some(SysDCData::new(name, member))
    }

    /**
     * <module> ::= module <id> \{ <function_list, delimiter=None> \}
     */
    fn parse_module(&mut self, namespace: &Name) -> Option<SysDCModule> {
        // module
        self.tokenizer.expect(TokenKind::Module)?;

        // <id>
        let name = Name::new(namespace, self.tokenizer.request(TokenKind::Identifier).get_id());

        // \{ <function_list, delimiter=None> \}
        self.tokenizer.request(TokenKind::BracketBegin);
        let functions = parse_list!(self.parse_function(&name));
        self.tokenizer.request(TokenKind::BracketEnd);

        Some(SysDCModule::new(name, functions))
    }

    /**
     * <function> ::= <id> <id_type_mapping_var_list, delimiter=,> -> <id> \{ <function_body> \}
     */
    fn parse_function(&mut self, namespace: &Name) -> Option<SysDCFunction> {
        // <id>
        let name_token = self.tokenizer.expect(TokenKind::Identifier)?;
        let name = Name::new(namespace, name_token.get_id());

        // <id_type_mapping_var_list, delimiter=,>
        self.tokenizer.request(TokenKind::ParenthesisBegin);
        let args = parse_list!(self.parse_id_type_mapping_var(&name), TokenKind::Separater);
        self.tokenizer.request(TokenKind::ParenthesisEnd);

        // -> <id>
        self.tokenizer.request(TokenKind::Allow);
        let return_type = SysDCType::from(&name, self.tokenizer.request(TokenKind::Identifier).get_id());   // TODO: Checker

        // \{ <function_body> \}
        self.tokenizer.request(TokenKind::BracketBegin);
        let (return_name, spawns) = self.parse_function_body(&name);
        self.tokenizer.request(TokenKind::BracketEnd);

        Some(SysDCFunction::new(name, args, (return_name, return_type), spawns))
    }

    /**
     * <function_body> = <annotation_list, delimiter=''>
     */
    fn parse_function_body(&mut self, namespace: &Name) -> (Name, Vec<SysDCSpawn>) {
        let mut returns: Option<Name> = None;
        let mut spawns = vec!();
        while let Some(annotation) = self.parse_annotation(namespace) {
            match annotation {
                SysDCAnnotation::Return(ret) => {
                    if returns.is_some() {
                        panic!("[ERROR] Annotation \"return\" is multiple defined")
                    }
                    returns = Some(ret)
                }
                SysDCAnnotation::Spawn(spawn) => spawns.push(spawn),
            }
        }
        if returns.is_none() {
            panic!("[ERROR] Annotation \"return\" is not defined");
        }
        (returns.unwrap(), spawns)
    }

    /**
     * <annotation> = <attribute_list, delimiter=''> @ <id> <body: annotationによって変化>
     */
    fn parse_annotation(&mut self, namespace: &Name) -> Option<SysDCAnnotation> {
        // <attribute_list, delimiter=''>
        let attributes = parse_list!(self.parse_attribute(namespace));

        // @
        if self.tokenizer.expect(TokenKind::AtMark).is_none() {
            if attributes.len() > 0 {
                panic!("[ERROR] Attributes found, but annotation not found");
            }
            return None;
        }

        // <id>
        let annotation = self.tokenizer.request(TokenKind::Identifier).get_id();
        match annotation.as_str() {
            "spawn" => {
                let spawn_result = self.parse_id_type_mapping_var(namespace);
                if spawn_result.is_none() {
                    panic!("[ERROR] Missing to specify the result of spawn");
                }

                let mut uses = vec!();
                for (attr, var_list) in attributes {
                    for (name, types) in var_list {
                        match attr.as_str() {
                            "use" => uses.push(SysDCSpawnChild::new_use(name, types)),
                            attr => panic!("[ERROR] Attribute \"{}\" is invalid on \"spawn\" attribute", attr)
                        }
                    }
                }

                Some(SysDCAnnotation::new_spawn(spawn_result.unwrap(), uses))
            },
            "return" => {
                let returns = self.tokenizer.request(TokenKind::Identifier).get_id();
                Some(SysDCAnnotation::new_return(Name::new(namespace, returns)))
            }
            annotation => panic!("[ERROR] Annotation \"{}\" is invalid", annotation)
        }
    }

    /**
     * <attribute> = \+ <id> <var_list, delimiter=','>
     */
    fn parse_attribute(&mut self, namespace: &Name) -> Option<(String, Vec<(Name, SysDCType)>)> {
        // \+
        self.tokenizer.expect(TokenKind::Plus)?;

        // <id>
        let attribute = self.tokenizer.request(TokenKind::Identifier).get_id();
        match attribute.as_str() {
            "use" => {},
            attribute => panic!("[ERROR] Attribute \"{}\" is invalid", attribute)
        }

        // <var_list, delimiter=','>
        let var_list = parse_list!(self.parse_var(namespace), TokenKind::Separater);

        Some((attribute, var_list))
    }

    /**
     * <var> ::= <id_list, delimiter=.>
     */
    fn parse_var(&mut self, namespace: &Name) -> Option<(Name, SysDCType)> {
        // <id_list, delimiter=,>
        let name_elems = parse_list!(self.tokenizer.expect(TokenKind::Identifier), TokenKind::Accessor);
        let var = name_elems.iter().map(|x| x.get_id()).collect::<Vec<String>>().join(".");
        match var.len() {
            0 => None,
            _ => Some((Name::new(namespace, var), SysDCType::UnsolvedNoHint))
        }
    }

    /**
     * <id_type_mapping_var> ::= <id> : <id> 
     */
    fn parse_id_type_mapping_var(&mut self, namespace: &Name) -> Option<(Name, SysDCType)> {
        // <id> : <id>
        let id1 = self.tokenizer.expect(TokenKind::Identifier)?.get_id();
        self.tokenizer.request(TokenKind::Mapping);
        let id2 = self.tokenizer.request(TokenKind::Identifier).get_id();
        Some((Name::new(namespace, id1), SysDCType::from(namespace, id2)))
    }
}

#[cfg(test)]
mod test {
    use super::Parser;
    use super::super::name::Name;
    use super::super::types::SysDCType;
    use super::super::token::Tokenizer;
    use super::super::structure::{ SysDCUnit, SysDCData, SysDCModule, SysDCFunction, SysDCSpawn, SysDCSpawnChild };
    
    #[test]
    fn data_empty_ok() {
        let program = "
            data A {}
            data B{}
            data C{   

            }
            data D
            {}
            data
            E
            {

            }
        ";

        let name = generate_name_for_test();

        let data = vec!(
            SysDCData::new(Name::new(&name, "A".to_string()), vec!()),
            SysDCData::new(Name::new(&name, "B".to_string()), vec!()),
            SysDCData::new(Name::new(&name, "C".to_string()), vec!()),
            SysDCData::new(Name::new(&name, "D".to_string()), vec!()),
            SysDCData::new(Name::new(&name, "E".to_string()), vec!())
        );
        let unit = SysDCUnit::new(name, data, vec!());

        compare_unit(program, unit);
    }

    #[test]
    fn data_has_member_ok() {
        let program = "
            data Box {
                x: i32,
                y: UserDefinedData,
            }
        ";

        let name = generate_name_for_test();
        let name_box = Name::new(&name, "Box".to_string());

        let member = vec!(
            (Name::new(&name_box, "x".to_string()), SysDCType::Int32),
            (Name::new(&name_box, "y".to_string()), SysDCType::from(&name_box, "UserDefinedData".to_string()))
        );
        let data = SysDCData::new(name_box, member);
        let unit = SysDCUnit::new(name, vec!(data), vec!());

        compare_unit(program, unit);
    }

    #[test]
    #[should_panic]
    fn data_has_illegal_member_def_1() {
        let program = "
            data Box {
                x: i32
                y: i32
            }
        ";
        parse(program);
    }

    #[test]
    #[should_panic]
    fn data_has_illegal_member_def_2() {
        let program = "
            data Box {
                x: i32,
                y:
            }
        ";
        parse(program);
    }

    #[test]
    #[should_panic]
    fn data_has_illegal_member_def_3() {
        let program = "
            data Box
                x: i32,
                y: i32
        ";
        parse(program);
    }

    #[test]
    fn module_empty_ok() {
        let program = "
            module A {}
            module B{}
            module C{   

            }
            module D
            {}
            module
            E
            {

            }
        ";

        let name = generate_name_for_test();

        let module = vec!(
            SysDCModule::new(Name::new(&name, "A".to_string()), vec!()),
            SysDCModule::new(Name::new(&name, "B".to_string()), vec!()),
            SysDCModule::new(Name::new(&name, "C".to_string()), vec!()),
            SysDCModule::new(Name::new(&name, "D".to_string()), vec!()),
            SysDCModule::new(Name::new(&name, "E".to_string()), vec!())
        );
        let unit = SysDCUnit::new(name, vec!(), module);

        compare_unit(program, unit);
    }

    #[test]
    fn function_only_has_return() {
        let program = "
            module BoxModule {
                new() -> Box {
                    @return box
                }
            }
        ";

        let name = generate_name_for_test();
        let name_module = Name::new(&name, "BoxModule".to_string());
        let name_func = Name::new(&name_module, "new".to_string());
        let name_func_ret = Name::new(&name_func, "box".to_string());

        let func_returns = (name_func_ret, SysDCType::from(&name_func, "Box".to_string()));
        let func = SysDCFunction::new(name_func, vec!(), func_returns, vec!());

        let module = SysDCModule::new(name_module, vec!(func));

        let unit = SysDCUnit::new(name, vec!(), vec!(module));

        compare_unit(program, unit);
    }

    #[test]
    fn function_has_return_and_spawn() {
        let program = "
            module BoxModule {
                new() -> Box {
                    @return box

                    @spawn box: Box
                }
            }
        ";

        let name = generate_name_for_test();
        let name_module = Name::new(&name, "BoxModule".to_string());
        let name_func = Name::new(&name_module, "new".to_string());
        let name_func_spawn_box = Name::new(&name_func, "box".to_string());
        let name_func_ret = Name::new(&name_func, "box".to_string());

        let func_spawns = vec!(
            SysDCSpawn::new((name_func_spawn_box, SysDCType::from(&name_func, "Box".to_string())), vec!())
        );
        let func_returns = (name_func_ret, SysDCType::from(&name_func, "Box".to_string()));
        let func = SysDCFunction::new(name_func, vec!(), func_returns, func_spawns);

        let module = SysDCModule::new(name_module, vec!(func));

        let unit = SysDCUnit::new(name, vec!(), vec!(module));

        compare_unit(program, unit);
    }

    #[test]
    fn function_has_full() {
        let program = "
            module BoxModule {
                move(box: Box, dx: i32, dy: i32) -> Box {
                    @return movedBox

                    +use box.x, box.y
                    +use dx, dy
                    @spawn movedBox: Box
                }
            }
        ";

        let name = generate_name_for_test();
        let name_module = Name::new(&name, "BoxModule".to_string());
        let name_func = Name::new(&name_module, "move".to_string());
        let name_func_arg_box = Name::new(&name_func, "box".to_string());
        let name_func_arg_dx = Name::new(&name_func, "dx".to_string());
        let name_func_arg_dy = Name::new(&name_func, "dy".to_string());
        let name_func_spawn_box = Name::new(&name_func, "movedBox".to_string());
        let name_func_spawn_use_box_x = Name::new(&name_func, "box.x".to_string());
        let name_func_spawn_use_box_y = Name::new(&name_func, "box.y".to_string());
        let name_func_spawn_use_dx = Name::new(&name_func, "dx".to_string());
        let name_func_spawn_use_dy = Name::new(&name_func, "dy".to_string());
        let name_func_ret = Name::new(&name_func, "movedBox".to_string());

        let func_args = vec!(
            (name_func_arg_box, SysDCType::from(&name_func, "Box".to_string())),
            (name_func_arg_dx, SysDCType::Int32),
            (name_func_arg_dy, SysDCType::Int32)
        );
        let func_spawns = vec!(
            SysDCSpawn::new((name_func_spawn_box, SysDCType::from(&name_func, "Box".to_string())), vec!(
                SysDCSpawnChild::new_use(name_func_spawn_use_box_x, SysDCType::UnsolvedNoHint),
                SysDCSpawnChild::new_use(name_func_spawn_use_box_y, SysDCType::UnsolvedNoHint),
                SysDCSpawnChild::new_use(name_func_spawn_use_dx, SysDCType::UnsolvedNoHint),
                SysDCSpawnChild::new_use(name_func_spawn_use_dy, SysDCType::UnsolvedNoHint)
            ))
        );
        let func_returns = (name_func_ret, SysDCType::from(&name_func, "Box".to_string()));
        let func = SysDCFunction::new(name_func, func_args, func_returns, func_spawns);

        let module = SysDCModule::new(name_module, vec!(func));

        let unit = SysDCUnit::new(name, vec!(), vec!(module));

        compare_unit(program, unit);
    }

    #[test]
    #[should_panic]
    fn illegal_function_1() {
        let program = "
            module BoxModule {
                move() -> {

                }
            }
        ";
        parse(program);
    }

    #[test]
    #[should_panic]
    fn illegal_function_2() {
        let program = "
            module BoxModule {
                move(box: Box, dx: i32, dy: ) -> i32 {

                }
            }
        ";
        parse(program);
    }

    #[test]
    #[should_panic]
    fn illegal_function_3() {
        let program = "
            module BoxModule {
                move() {

                }
            }
        ";
        parse(program);
    }

    #[test]
    fn full() {
        let program = "
            data Box {
                x: i32,
                y: i32
            }

            module BoxModule {
                move(box: Box, dx: i32, dy: i32) -> Box {
                    @return movedBox

                    +use box.x, box.y, dx, dy
                    @spawn movedBox: Box
                }
            }
        ";

        let name = generate_name_for_test();
        let name_data = Name::new(&name, "Box".to_string());
        let name_data_x = Name::new(&name_data, "x".to_string());
        let name_data_y = Name::new(&name_data, "y".to_string());
        let name_module = Name::new(&name, "BoxModule".to_string());
        let name_func = Name::new(&name_module, "move".to_string());
        let name_func_arg_box = Name::new(&name_func, "box".to_string());
        let name_func_arg_dx = Name::new(&name_func, "dx".to_string());
        let name_func_arg_dy = Name::new(&name_func, "dy".to_string());
        let name_func_spawn_box = Name::new(&name_func, "movedBox".to_string());
        let name_func_spawn_use_box_x = Name::new(&name_func, "box.x".to_string());
        let name_func_spawn_use_box_y = Name::new(&name_func, "box.y".to_string());
        let name_func_spawn_use_dx = Name::new(&name_func, "dx".to_string());
        let name_func_spawn_use_dy = Name::new(&name_func, "dy".to_string());
        let name_func_ret = Name::new(&name_func, "movedBox".to_string());

        let func_args = vec!(
            (name_func_arg_box, SysDCType::from(&name_func, "Box".to_string())),
            (name_func_arg_dx, SysDCType::Int32),
            (name_func_arg_dy, SysDCType::Int32)
        );
        let func_spawns = vec!(
            SysDCSpawn::new((name_func_spawn_box, SysDCType::from(&name_func, "Box".to_string())), vec!(
                SysDCSpawnChild::new_use(name_func_spawn_use_box_x, SysDCType::UnsolvedNoHint),
                SysDCSpawnChild::new_use(name_func_spawn_use_box_y, SysDCType::UnsolvedNoHint),
                SysDCSpawnChild::new_use(name_func_spawn_use_dx, SysDCType::UnsolvedNoHint),
                SysDCSpawnChild::new_use(name_func_spawn_use_dy, SysDCType::UnsolvedNoHint)
            ))
        );
        let func_returns = (name_func_ret, SysDCType::from(&name_func, "Box".to_string()));
        let func = SysDCFunction::new(name_func, func_args, func_returns, func_spawns);

        let module = SysDCModule::new(name_module, vec!(func));

        let data_members = vec!(
            (name_data_x, SysDCType::Int32),
            (name_data_y, SysDCType::Int32)
        );
        let data = SysDCData::new(name_data, data_members);

        let unit = SysDCUnit::new(name, vec!(data), vec!(module));

        compare_unit(program, unit);
    }


    fn generate_name_for_test() -> Name {
        Name::new(&Name::new_root(), "test".to_string())
    }

    fn compare_unit(program: &str, unit: SysDCUnit) {
        assert_eq!(format!("{:?}", parse(program)), format!("{:?}", unit));
    }

    fn parse(program: &str) -> SysDCUnit {
        let program = program.to_string();
        let tokenizer = Tokenizer::new(&program);
        let mut parser = Parser::new(tokenizer);
        parser.parse(&generate_name_for_test())
    }
}
