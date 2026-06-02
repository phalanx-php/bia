use mago_span::{HasSpan, Span};
use mago_syntax::ast::{
    ClassLikeConstantSelector, ClassLikeMemberSelector, DirectVariable, Expression, Function,
    FunctionCall, Method, MethodCall, Namespace, NullSafeMethodCall, Program, StaticMethodCall,
};
use mago_syntax::walker::{Walker, walk_program};

use super::records::{CodeNodeRecord, ReferenceRecord};
use super::{AnalysisSource, identifier_name, local_name, member_name, qualified_name};

pub(crate) fn collect_search_projection(
    source: &AnalysisSource,
    program: &Program<'_>,
) -> (Vec<CodeNodeRecord>, Vec<ReferenceRecord>) {
    let mut context = ProjectionContext {
        source,
        namespace: None,
        context_stack: Vec::new(),
        nodes: Vec::new(),
        references: Vec::new(),
    };

    walk_program(&ProjectionWalker, program, &mut context);

    (context.nodes, context.references)
}

struct ProjectionContext<'source> {
    source: &'source AnalysisSource,
    namespace: Option<String>,
    context_stack: Vec<String>,
    nodes: Vec<CodeNodeRecord>,
    references: Vec<ReferenceRecord>,
}

struct ProjectionWalker;

impl<'ast, 'arena, 'source> Walker<'ast, 'arena, ProjectionContext<'source>> for ProjectionWalker {
    fn walk_in_namespace(
        &self,
        namespace: &'ast Namespace<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.namespace = namespace.name.as_ref().map(identifier_name);
    }

    fn walk_out_namespace(
        &self,
        _namespace: &'ast Namespace<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.namespace = None;
    }

    fn walk_in_class(
        &self,
        class: &'ast mago_syntax::ast::Class<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = local_name(&class.name);
        let fqn = qualified_name(context.namespace.as_deref(), &name);

        context.push_node("class", Some(name), class.span());
        context.context_stack.push(fqn);
    }

    fn walk_out_class(
        &self,
        _class: &'ast mago_syntax::ast::Class<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.context_stack.pop();
    }

    fn walk_in_interface(
        &self,
        interface: &'ast mago_syntax::ast::Interface<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = local_name(&interface.name);
        let fqn = qualified_name(context.namespace.as_deref(), &name);

        context.push_node("interface", Some(name), interface.span());
        context.context_stack.push(fqn);
    }

    fn walk_out_interface(
        &self,
        _interface: &'ast mago_syntax::ast::Interface<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.context_stack.pop();
    }

    fn walk_in_trait(
        &self,
        r#trait: &'ast mago_syntax::ast::Trait<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = local_name(&r#trait.name);
        let fqn = qualified_name(context.namespace.as_deref(), &name);

        context.push_node("trait", Some(name), r#trait.span());
        context.context_stack.push(fqn);
    }

    fn walk_out_trait(
        &self,
        _trait: &'ast mago_syntax::ast::Trait<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.context_stack.pop();
    }

    fn walk_in_enum(
        &self,
        r#enum: &'ast mago_syntax::ast::Enum<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = local_name(&r#enum.name);
        let fqn = qualified_name(context.namespace.as_deref(), &name);

        context.push_node("enum", Some(name), r#enum.span());
        context.context_stack.push(fqn);
    }

    fn walk_out_enum(
        &self,
        _enum: &'ast mago_syntax::ast::Enum<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.context_stack.pop();
    }

    fn walk_in_function(
        &self,
        function: &'ast Function<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = local_name(&function.name);
        let fqn = qualified_name(context.namespace.as_deref(), &name);

        context.push_node("function", Some(name), function.span());
        context.context_stack.push(fqn);
    }

    fn walk_out_function(
        &self,
        _function: &'ast Function<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.context_stack.pop();
    }

    fn walk_in_method(
        &self,
        method: &'ast Method<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = local_name(&method.name);
        let fqn = context.current_context().map_or_else(
            || name.clone(),
            |declaring_type| member_name(&declaring_type, &name),
        );

        context.push_node("method", Some(name), method.span());
        context.context_stack.push(fqn);
    }

    fn walk_out_method(
        &self,
        _method: &'ast Method<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        context.context_stack.pop();
    }

    fn walk_in_direct_variable(
        &self,
        variable: &'ast DirectVariable<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = String::from_utf8_lossy(variable.name).into_owned();

        context.push_reference("variable", name, variable.span(), None);
    }

    fn walk_in_function_call(
        &self,
        call: &'ast FunctionCall<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        if let Some(name) = expression_identifier(call.function) {
            context.push_reference("function", name.clone(), call.function.span(), None);
            context.push_node("function-call", Some(name), call.span());
        }
    }

    fn walk_in_method_call(
        &self,
        call: &'ast MethodCall<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        if let Some(name) = member_selector_name(&call.method) {
            let receiver = context.source.map.source_text(call.object.span());

            context.push_reference("method", name.clone(), call.method.span(), receiver);
            context.push_node("method-call", Some(name), call.span());
        }
    }

    fn walk_in_null_safe_method_call(
        &self,
        call: &'ast NullSafeMethodCall<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        if let Some(name) = member_selector_name(&call.method) {
            let receiver = context.source.map.source_text(call.object.span());

            context.push_reference("method", name.clone(), call.method.span(), receiver);
            context.push_node("null-safe-method-call", Some(name), call.span());
        }
    }

    fn walk_in_static_method_call(
        &self,
        call: &'ast StaticMethodCall<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        if let Some(name) = member_selector_name(&call.method) {
            let receiver = context.source.map.source_text(call.class.span());

            context.push_reference("static-method", name.clone(), call.method.span(), receiver);
            context.push_node("static-method-call", Some(name), call.span());
        }
    }

    fn walk_in_constant_access(
        &self,
        access: &'ast mago_syntax::ast::ConstantAccess<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        let name = identifier_name(&access.name);

        context.push_reference("constant", name, access.span(), None);
    }

    fn walk_in_property_access(
        &self,
        access: &'ast mago_syntax::ast::PropertyAccess<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        if let Some(name) = member_selector_name(&access.property) {
            let receiver = context.source.map.source_text(access.object.span());

            context.push_reference("property", name.clone(), access.property.span(), receiver);
            context.push_node("property-access", Some(name), access.span());
        }
    }

    fn walk_in_null_safe_property_access(
        &self,
        access: &'ast mago_syntax::ast::NullSafePropertyAccess<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        if let Some(name) = member_selector_name(&access.property) {
            let receiver = context.source.map.source_text(access.object.span());

            context.push_reference("property", name.clone(), access.property.span(), receiver);
            context.push_node("null-safe-property-access", Some(name), access.span());
        }
    }

    fn walk_in_class_constant_access(
        &self,
        access: &'ast mago_syntax::ast::ClassConstantAccess<'arena>,
        context: &mut ProjectionContext<'source>,
    ) {
        if let Some(name) = class_constant_selector_name(&access.constant) {
            let receiver = context.source.map.source_text(access.class.span());

            context.push_reference(
                "class-constant",
                name.clone(),
                access.constant.span(),
                receiver,
            );
            context.push_node("class-constant-access", Some(name), access.span());
        }
    }
}

impl ProjectionContext<'_> {
    fn current_context(&self) -> Option<String> {
        self.context_stack.last().cloned()
    }

    fn push_node(&mut self, kind: &'static str, name: Option<String>, span: Span) {
        if !self.source.map.contains_source_span(span) {
            return;
        }

        self.nodes.push(CodeNodeRecord {
            kind,
            name,
            span: self.source.map.span_record(span),
            context: self.current_context(),
            file: None,
        });
    }

    fn push_reference(
        &mut self,
        kind: &'static str,
        name: String,
        span: Span,
        receiver: Option<String>,
    ) {
        if !self.source.map.contains_source_span(span) {
            return;
        }

        self.references.push(ReferenceRecord {
            kind,
            name,
            receiver,
            span: self.source.map.span_record(span),
            context: self.current_context(),
            file: None,
        });
    }
}

fn expression_identifier(expression: &Expression<'_>) -> Option<String> {
    match expression.unparenthesized() {
        Expression::Identifier(identifier) => Some(identifier_name(identifier)),
        _ => None,
    }
}

fn member_selector_name(selector: &ClassLikeMemberSelector<'_>) -> Option<String> {
    match selector {
        ClassLikeMemberSelector::Identifier(identifier) => Some(local_name(identifier)),
        _ => None,
    }
}

fn class_constant_selector_name(selector: &ClassLikeConstantSelector<'_>) -> Option<String> {
    match selector {
        ClassLikeConstantSelector::Identifier(identifier) => Some(local_name(identifier)),
        _ => None,
    }
}
