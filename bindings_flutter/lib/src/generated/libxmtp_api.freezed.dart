// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'libxmtp_api.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

T _$identity<T>(T value) => value;

final _privateConstructorUsedError = UnsupportedError(
    'It seems like you constructed your class using `MyClass._()`. This constructor is only meant to be used by freezed and you are not supposed to need it nor use it.\nPlease check the documentation here for more information: https://github.com/rrousselGit/freezed#custom-getters-and-methods');

/// @nodoc
mixin _$CreatedClient {
  Object get field0 => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(Client field0) ready,
    required TResult Function(SignatureRequiredClient field0) requiresSignature,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(Client field0)? ready,
    TResult? Function(SignatureRequiredClient field0)? requiresSignature,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(Client field0)? ready,
    TResult Function(SignatureRequiredClient field0)? requiresSignature,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CreatedClient_Ready value) ready,
    required TResult Function(CreatedClient_RequiresSignature value)
        requiresSignature,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CreatedClient_Ready value)? ready,
    TResult? Function(CreatedClient_RequiresSignature value)? requiresSignature,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CreatedClient_Ready value)? ready,
    TResult Function(CreatedClient_RequiresSignature value)? requiresSignature,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $CreatedClientCopyWith<$Res> {
  factory $CreatedClientCopyWith(
          CreatedClient value, $Res Function(CreatedClient) then) =
      _$CreatedClientCopyWithImpl<$Res, CreatedClient>;
}

/// @nodoc
class _$CreatedClientCopyWithImpl<$Res, $Val extends CreatedClient>
    implements $CreatedClientCopyWith<$Res> {
  _$CreatedClientCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;
}

/// @nodoc
abstract class _$$CreatedClient_ReadyImplCopyWith<$Res> {
  factory _$$CreatedClient_ReadyImplCopyWith(_$CreatedClient_ReadyImpl value,
          $Res Function(_$CreatedClient_ReadyImpl) then) =
      __$$CreatedClient_ReadyImplCopyWithImpl<$Res>;
  @useResult
  $Res call({Client field0});
}

/// @nodoc
class __$$CreatedClient_ReadyImplCopyWithImpl<$Res>
    extends _$CreatedClientCopyWithImpl<$Res, _$CreatedClient_ReadyImpl>
    implements _$$CreatedClient_ReadyImplCopyWith<$Res> {
  __$$CreatedClient_ReadyImplCopyWithImpl(_$CreatedClient_ReadyImpl _value,
      $Res Function(_$CreatedClient_ReadyImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$CreatedClient_ReadyImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as Client,
    ));
  }
}

/// @nodoc

class _$CreatedClient_ReadyImpl implements CreatedClient_Ready {
  const _$CreatedClient_ReadyImpl(this.field0);

  @override
  final Client field0;

  @override
  String toString() {
    return 'CreatedClient.ready(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CreatedClient_ReadyImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CreatedClient_ReadyImplCopyWith<_$CreatedClient_ReadyImpl> get copyWith =>
      __$$CreatedClient_ReadyImplCopyWithImpl<_$CreatedClient_ReadyImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(Client field0) ready,
    required TResult Function(SignatureRequiredClient field0) requiresSignature,
  }) {
    return ready(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(Client field0)? ready,
    TResult? Function(SignatureRequiredClient field0)? requiresSignature,
  }) {
    return ready?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(Client field0)? ready,
    TResult Function(SignatureRequiredClient field0)? requiresSignature,
    required TResult orElse(),
  }) {
    if (ready != null) {
      return ready(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CreatedClient_Ready value) ready,
    required TResult Function(CreatedClient_RequiresSignature value)
        requiresSignature,
  }) {
    return ready(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CreatedClient_Ready value)? ready,
    TResult? Function(CreatedClient_RequiresSignature value)? requiresSignature,
  }) {
    return ready?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CreatedClient_Ready value)? ready,
    TResult Function(CreatedClient_RequiresSignature value)? requiresSignature,
    required TResult orElse(),
  }) {
    if (ready != null) {
      return ready(this);
    }
    return orElse();
  }
}

abstract class CreatedClient_Ready implements CreatedClient {
  const factory CreatedClient_Ready(final Client field0) =
      _$CreatedClient_ReadyImpl;

  @override
  Client get field0;
  @JsonKey(ignore: true)
  _$$CreatedClient_ReadyImplCopyWith<_$CreatedClient_ReadyImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$CreatedClient_RequiresSignatureImplCopyWith<$Res> {
  factory _$$CreatedClient_RequiresSignatureImplCopyWith(
          _$CreatedClient_RequiresSignatureImpl value,
          $Res Function(_$CreatedClient_RequiresSignatureImpl) then) =
      __$$CreatedClient_RequiresSignatureImplCopyWithImpl<$Res>;
  @useResult
  $Res call({SignatureRequiredClient field0});
}

/// @nodoc
class __$$CreatedClient_RequiresSignatureImplCopyWithImpl<$Res>
    extends _$CreatedClientCopyWithImpl<$Res,
        _$CreatedClient_RequiresSignatureImpl>
    implements _$$CreatedClient_RequiresSignatureImplCopyWith<$Res> {
  __$$CreatedClient_RequiresSignatureImplCopyWithImpl(
      _$CreatedClient_RequiresSignatureImpl _value,
      $Res Function(_$CreatedClient_RequiresSignatureImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$CreatedClient_RequiresSignatureImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as SignatureRequiredClient,
    ));
  }
}

/// @nodoc

class _$CreatedClient_RequiresSignatureImpl
    implements CreatedClient_RequiresSignature {
  const _$CreatedClient_RequiresSignatureImpl(this.field0);

  @override
  final SignatureRequiredClient field0;

  @override
  String toString() {
    return 'CreatedClient.requiresSignature(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$CreatedClient_RequiresSignatureImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$CreatedClient_RequiresSignatureImplCopyWith<
          _$CreatedClient_RequiresSignatureImpl>
      get copyWith => __$$CreatedClient_RequiresSignatureImplCopyWithImpl<
          _$CreatedClient_RequiresSignatureImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(Client field0) ready,
    required TResult Function(SignatureRequiredClient field0) requiresSignature,
  }) {
    return requiresSignature(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(Client field0)? ready,
    TResult? Function(SignatureRequiredClient field0)? requiresSignature,
  }) {
    return requiresSignature?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(Client field0)? ready,
    TResult Function(SignatureRequiredClient field0)? requiresSignature,
    required TResult orElse(),
  }) {
    if (requiresSignature != null) {
      return requiresSignature(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(CreatedClient_Ready value) ready,
    required TResult Function(CreatedClient_RequiresSignature value)
        requiresSignature,
  }) {
    return requiresSignature(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(CreatedClient_Ready value)? ready,
    TResult? Function(CreatedClient_RequiresSignature value)? requiresSignature,
  }) {
    return requiresSignature?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(CreatedClient_Ready value)? ready,
    TResult Function(CreatedClient_RequiresSignature value)? requiresSignature,
    required TResult orElse(),
  }) {
    if (requiresSignature != null) {
      return requiresSignature(this);
    }
    return orElse();
  }
}

abstract class CreatedClient_RequiresSignature implements CreatedClient {
  const factory CreatedClient_RequiresSignature(
          final SignatureRequiredClient field0) =
      _$CreatedClient_RequiresSignatureImpl;

  @override
  SignatureRequiredClient get field0;
  @JsonKey(ignore: true)
  _$$CreatedClient_RequiresSignatureImplCopyWith<
          _$CreatedClient_RequiresSignatureImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
mixin _$XmtpError {
  RustOpaque get field0 => throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(ApiError field0) apiError,
    required TResult Function(ClientBuilderError field0) clientBuilderError,
    required TResult Function(XmtpMlsClientClientError field0) clientError,
    required TResult Function(StorageError field0) storageError,
    required TResult Function(AnyhowError field0) generic,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(ApiError field0)? apiError,
    TResult? Function(ClientBuilderError field0)? clientBuilderError,
    TResult? Function(XmtpMlsClientClientError field0)? clientError,
    TResult? Function(StorageError field0)? storageError,
    TResult? Function(AnyhowError field0)? generic,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(ApiError field0)? apiError,
    TResult Function(ClientBuilderError field0)? clientBuilderError,
    TResult Function(XmtpMlsClientClientError field0)? clientError,
    TResult Function(StorageError field0)? storageError,
    TResult Function(AnyhowError field0)? generic,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(XmtpError_ApiError value) apiError,
    required TResult Function(XmtpError_ClientBuilderError value)
        clientBuilderError,
    required TResult Function(XmtpError_ClientError value) clientError,
    required TResult Function(XmtpError_StorageError value) storageError,
    required TResult Function(XmtpError_Generic value) generic,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(XmtpError_ApiError value)? apiError,
    TResult? Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult? Function(XmtpError_ClientError value)? clientError,
    TResult? Function(XmtpError_StorageError value)? storageError,
    TResult? Function(XmtpError_Generic value)? generic,
  }) =>
      throw _privateConstructorUsedError;
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(XmtpError_ApiError value)? apiError,
    TResult Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult Function(XmtpError_ClientError value)? clientError,
    TResult Function(XmtpError_StorageError value)? storageError,
    TResult Function(XmtpError_Generic value)? generic,
    required TResult orElse(),
  }) =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class $XmtpErrorCopyWith<$Res> {
  factory $XmtpErrorCopyWith(XmtpError value, $Res Function(XmtpError) then) =
      _$XmtpErrorCopyWithImpl<$Res, XmtpError>;
}

/// @nodoc
class _$XmtpErrorCopyWithImpl<$Res, $Val extends XmtpError>
    implements $XmtpErrorCopyWith<$Res> {
  _$XmtpErrorCopyWithImpl(this._value, this._then);

  // ignore: unused_field
  final $Val _value;
  // ignore: unused_field
  final $Res Function($Val) _then;
}

/// @nodoc
abstract class _$$XmtpError_ApiErrorImplCopyWith<$Res> {
  factory _$$XmtpError_ApiErrorImplCopyWith(_$XmtpError_ApiErrorImpl value,
          $Res Function(_$XmtpError_ApiErrorImpl) then) =
      __$$XmtpError_ApiErrorImplCopyWithImpl<$Res>;
  @useResult
  $Res call({ApiError field0});
}

/// @nodoc
class __$$XmtpError_ApiErrorImplCopyWithImpl<$Res>
    extends _$XmtpErrorCopyWithImpl<$Res, _$XmtpError_ApiErrorImpl>
    implements _$$XmtpError_ApiErrorImplCopyWith<$Res> {
  __$$XmtpError_ApiErrorImplCopyWithImpl(_$XmtpError_ApiErrorImpl _value,
      $Res Function(_$XmtpError_ApiErrorImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$XmtpError_ApiErrorImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as ApiError,
    ));
  }
}

/// @nodoc

class _$XmtpError_ApiErrorImpl implements XmtpError_ApiError {
  const _$XmtpError_ApiErrorImpl(this.field0);

  @override
  final ApiError field0;

  @override
  String toString() {
    return 'XmtpError.apiError(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$XmtpError_ApiErrorImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$XmtpError_ApiErrorImplCopyWith<_$XmtpError_ApiErrorImpl> get copyWith =>
      __$$XmtpError_ApiErrorImplCopyWithImpl<_$XmtpError_ApiErrorImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(ApiError field0) apiError,
    required TResult Function(ClientBuilderError field0) clientBuilderError,
    required TResult Function(XmtpMlsClientClientError field0) clientError,
    required TResult Function(StorageError field0) storageError,
    required TResult Function(AnyhowError field0) generic,
  }) {
    return apiError(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(ApiError field0)? apiError,
    TResult? Function(ClientBuilderError field0)? clientBuilderError,
    TResult? Function(XmtpMlsClientClientError field0)? clientError,
    TResult? Function(StorageError field0)? storageError,
    TResult? Function(AnyhowError field0)? generic,
  }) {
    return apiError?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(ApiError field0)? apiError,
    TResult Function(ClientBuilderError field0)? clientBuilderError,
    TResult Function(XmtpMlsClientClientError field0)? clientError,
    TResult Function(StorageError field0)? storageError,
    TResult Function(AnyhowError field0)? generic,
    required TResult orElse(),
  }) {
    if (apiError != null) {
      return apiError(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(XmtpError_ApiError value) apiError,
    required TResult Function(XmtpError_ClientBuilderError value)
        clientBuilderError,
    required TResult Function(XmtpError_ClientError value) clientError,
    required TResult Function(XmtpError_StorageError value) storageError,
    required TResult Function(XmtpError_Generic value) generic,
  }) {
    return apiError(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(XmtpError_ApiError value)? apiError,
    TResult? Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult? Function(XmtpError_ClientError value)? clientError,
    TResult? Function(XmtpError_StorageError value)? storageError,
    TResult? Function(XmtpError_Generic value)? generic,
  }) {
    return apiError?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(XmtpError_ApiError value)? apiError,
    TResult Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult Function(XmtpError_ClientError value)? clientError,
    TResult Function(XmtpError_StorageError value)? storageError,
    TResult Function(XmtpError_Generic value)? generic,
    required TResult orElse(),
  }) {
    if (apiError != null) {
      return apiError(this);
    }
    return orElse();
  }
}

abstract class XmtpError_ApiError implements XmtpError {
  const factory XmtpError_ApiError(final ApiError field0) =
      _$XmtpError_ApiErrorImpl;

  @override
  ApiError get field0;
  @JsonKey(ignore: true)
  _$$XmtpError_ApiErrorImplCopyWith<_$XmtpError_ApiErrorImpl> get copyWith =>
      throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$XmtpError_ClientBuilderErrorImplCopyWith<$Res> {
  factory _$$XmtpError_ClientBuilderErrorImplCopyWith(
          _$XmtpError_ClientBuilderErrorImpl value,
          $Res Function(_$XmtpError_ClientBuilderErrorImpl) then) =
      __$$XmtpError_ClientBuilderErrorImplCopyWithImpl<$Res>;
  @useResult
  $Res call({ClientBuilderError field0});
}

/// @nodoc
class __$$XmtpError_ClientBuilderErrorImplCopyWithImpl<$Res>
    extends _$XmtpErrorCopyWithImpl<$Res, _$XmtpError_ClientBuilderErrorImpl>
    implements _$$XmtpError_ClientBuilderErrorImplCopyWith<$Res> {
  __$$XmtpError_ClientBuilderErrorImplCopyWithImpl(
      _$XmtpError_ClientBuilderErrorImpl _value,
      $Res Function(_$XmtpError_ClientBuilderErrorImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$XmtpError_ClientBuilderErrorImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as ClientBuilderError,
    ));
  }
}

/// @nodoc

class _$XmtpError_ClientBuilderErrorImpl
    implements XmtpError_ClientBuilderError {
  const _$XmtpError_ClientBuilderErrorImpl(this.field0);

  @override
  final ClientBuilderError field0;

  @override
  String toString() {
    return 'XmtpError.clientBuilderError(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$XmtpError_ClientBuilderErrorImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$XmtpError_ClientBuilderErrorImplCopyWith<
          _$XmtpError_ClientBuilderErrorImpl>
      get copyWith => __$$XmtpError_ClientBuilderErrorImplCopyWithImpl<
          _$XmtpError_ClientBuilderErrorImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(ApiError field0) apiError,
    required TResult Function(ClientBuilderError field0) clientBuilderError,
    required TResult Function(XmtpMlsClientClientError field0) clientError,
    required TResult Function(StorageError field0) storageError,
    required TResult Function(AnyhowError field0) generic,
  }) {
    return clientBuilderError(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(ApiError field0)? apiError,
    TResult? Function(ClientBuilderError field0)? clientBuilderError,
    TResult? Function(XmtpMlsClientClientError field0)? clientError,
    TResult? Function(StorageError field0)? storageError,
    TResult? Function(AnyhowError field0)? generic,
  }) {
    return clientBuilderError?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(ApiError field0)? apiError,
    TResult Function(ClientBuilderError field0)? clientBuilderError,
    TResult Function(XmtpMlsClientClientError field0)? clientError,
    TResult Function(StorageError field0)? storageError,
    TResult Function(AnyhowError field0)? generic,
    required TResult orElse(),
  }) {
    if (clientBuilderError != null) {
      return clientBuilderError(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(XmtpError_ApiError value) apiError,
    required TResult Function(XmtpError_ClientBuilderError value)
        clientBuilderError,
    required TResult Function(XmtpError_ClientError value) clientError,
    required TResult Function(XmtpError_StorageError value) storageError,
    required TResult Function(XmtpError_Generic value) generic,
  }) {
    return clientBuilderError(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(XmtpError_ApiError value)? apiError,
    TResult? Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult? Function(XmtpError_ClientError value)? clientError,
    TResult? Function(XmtpError_StorageError value)? storageError,
    TResult? Function(XmtpError_Generic value)? generic,
  }) {
    return clientBuilderError?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(XmtpError_ApiError value)? apiError,
    TResult Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult Function(XmtpError_ClientError value)? clientError,
    TResult Function(XmtpError_StorageError value)? storageError,
    TResult Function(XmtpError_Generic value)? generic,
    required TResult orElse(),
  }) {
    if (clientBuilderError != null) {
      return clientBuilderError(this);
    }
    return orElse();
  }
}

abstract class XmtpError_ClientBuilderError implements XmtpError {
  const factory XmtpError_ClientBuilderError(final ClientBuilderError field0) =
      _$XmtpError_ClientBuilderErrorImpl;

  @override
  ClientBuilderError get field0;
  @JsonKey(ignore: true)
  _$$XmtpError_ClientBuilderErrorImplCopyWith<
          _$XmtpError_ClientBuilderErrorImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$XmtpError_ClientErrorImplCopyWith<$Res> {
  factory _$$XmtpError_ClientErrorImplCopyWith(
          _$XmtpError_ClientErrorImpl value,
          $Res Function(_$XmtpError_ClientErrorImpl) then) =
      __$$XmtpError_ClientErrorImplCopyWithImpl<$Res>;
  @useResult
  $Res call({XmtpMlsClientClientError field0});
}

/// @nodoc
class __$$XmtpError_ClientErrorImplCopyWithImpl<$Res>
    extends _$XmtpErrorCopyWithImpl<$Res, _$XmtpError_ClientErrorImpl>
    implements _$$XmtpError_ClientErrorImplCopyWith<$Res> {
  __$$XmtpError_ClientErrorImplCopyWithImpl(_$XmtpError_ClientErrorImpl _value,
      $Res Function(_$XmtpError_ClientErrorImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$XmtpError_ClientErrorImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as XmtpMlsClientClientError,
    ));
  }
}

/// @nodoc

class _$XmtpError_ClientErrorImpl implements XmtpError_ClientError {
  const _$XmtpError_ClientErrorImpl(this.field0);

  @override
  final XmtpMlsClientClientError field0;

  @override
  String toString() {
    return 'XmtpError.clientError(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$XmtpError_ClientErrorImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$XmtpError_ClientErrorImplCopyWith<_$XmtpError_ClientErrorImpl>
      get copyWith => __$$XmtpError_ClientErrorImplCopyWithImpl<
          _$XmtpError_ClientErrorImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(ApiError field0) apiError,
    required TResult Function(ClientBuilderError field0) clientBuilderError,
    required TResult Function(XmtpMlsClientClientError field0) clientError,
    required TResult Function(StorageError field0) storageError,
    required TResult Function(AnyhowError field0) generic,
  }) {
    return clientError(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(ApiError field0)? apiError,
    TResult? Function(ClientBuilderError field0)? clientBuilderError,
    TResult? Function(XmtpMlsClientClientError field0)? clientError,
    TResult? Function(StorageError field0)? storageError,
    TResult? Function(AnyhowError field0)? generic,
  }) {
    return clientError?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(ApiError field0)? apiError,
    TResult Function(ClientBuilderError field0)? clientBuilderError,
    TResult Function(XmtpMlsClientClientError field0)? clientError,
    TResult Function(StorageError field0)? storageError,
    TResult Function(AnyhowError field0)? generic,
    required TResult orElse(),
  }) {
    if (clientError != null) {
      return clientError(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(XmtpError_ApiError value) apiError,
    required TResult Function(XmtpError_ClientBuilderError value)
        clientBuilderError,
    required TResult Function(XmtpError_ClientError value) clientError,
    required TResult Function(XmtpError_StorageError value) storageError,
    required TResult Function(XmtpError_Generic value) generic,
  }) {
    return clientError(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(XmtpError_ApiError value)? apiError,
    TResult? Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult? Function(XmtpError_ClientError value)? clientError,
    TResult? Function(XmtpError_StorageError value)? storageError,
    TResult? Function(XmtpError_Generic value)? generic,
  }) {
    return clientError?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(XmtpError_ApiError value)? apiError,
    TResult Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult Function(XmtpError_ClientError value)? clientError,
    TResult Function(XmtpError_StorageError value)? storageError,
    TResult Function(XmtpError_Generic value)? generic,
    required TResult orElse(),
  }) {
    if (clientError != null) {
      return clientError(this);
    }
    return orElse();
  }
}

abstract class XmtpError_ClientError implements XmtpError {
  const factory XmtpError_ClientError(final XmtpMlsClientClientError field0) =
      _$XmtpError_ClientErrorImpl;

  @override
  XmtpMlsClientClientError get field0;
  @JsonKey(ignore: true)
  _$$XmtpError_ClientErrorImplCopyWith<_$XmtpError_ClientErrorImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$XmtpError_StorageErrorImplCopyWith<$Res> {
  factory _$$XmtpError_StorageErrorImplCopyWith(
          _$XmtpError_StorageErrorImpl value,
          $Res Function(_$XmtpError_StorageErrorImpl) then) =
      __$$XmtpError_StorageErrorImplCopyWithImpl<$Res>;
  @useResult
  $Res call({StorageError field0});
}

/// @nodoc
class __$$XmtpError_StorageErrorImplCopyWithImpl<$Res>
    extends _$XmtpErrorCopyWithImpl<$Res, _$XmtpError_StorageErrorImpl>
    implements _$$XmtpError_StorageErrorImplCopyWith<$Res> {
  __$$XmtpError_StorageErrorImplCopyWithImpl(
      _$XmtpError_StorageErrorImpl _value,
      $Res Function(_$XmtpError_StorageErrorImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$XmtpError_StorageErrorImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as StorageError,
    ));
  }
}

/// @nodoc

class _$XmtpError_StorageErrorImpl implements XmtpError_StorageError {
  const _$XmtpError_StorageErrorImpl(this.field0);

  @override
  final StorageError field0;

  @override
  String toString() {
    return 'XmtpError.storageError(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$XmtpError_StorageErrorImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$XmtpError_StorageErrorImplCopyWith<_$XmtpError_StorageErrorImpl>
      get copyWith => __$$XmtpError_StorageErrorImplCopyWithImpl<
          _$XmtpError_StorageErrorImpl>(this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(ApiError field0) apiError,
    required TResult Function(ClientBuilderError field0) clientBuilderError,
    required TResult Function(XmtpMlsClientClientError field0) clientError,
    required TResult Function(StorageError field0) storageError,
    required TResult Function(AnyhowError field0) generic,
  }) {
    return storageError(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(ApiError field0)? apiError,
    TResult? Function(ClientBuilderError field0)? clientBuilderError,
    TResult? Function(XmtpMlsClientClientError field0)? clientError,
    TResult? Function(StorageError field0)? storageError,
    TResult? Function(AnyhowError field0)? generic,
  }) {
    return storageError?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(ApiError field0)? apiError,
    TResult Function(ClientBuilderError field0)? clientBuilderError,
    TResult Function(XmtpMlsClientClientError field0)? clientError,
    TResult Function(StorageError field0)? storageError,
    TResult Function(AnyhowError field0)? generic,
    required TResult orElse(),
  }) {
    if (storageError != null) {
      return storageError(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(XmtpError_ApiError value) apiError,
    required TResult Function(XmtpError_ClientBuilderError value)
        clientBuilderError,
    required TResult Function(XmtpError_ClientError value) clientError,
    required TResult Function(XmtpError_StorageError value) storageError,
    required TResult Function(XmtpError_Generic value) generic,
  }) {
    return storageError(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(XmtpError_ApiError value)? apiError,
    TResult? Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult? Function(XmtpError_ClientError value)? clientError,
    TResult? Function(XmtpError_StorageError value)? storageError,
    TResult? Function(XmtpError_Generic value)? generic,
  }) {
    return storageError?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(XmtpError_ApiError value)? apiError,
    TResult Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult Function(XmtpError_ClientError value)? clientError,
    TResult Function(XmtpError_StorageError value)? storageError,
    TResult Function(XmtpError_Generic value)? generic,
    required TResult orElse(),
  }) {
    if (storageError != null) {
      return storageError(this);
    }
    return orElse();
  }
}

abstract class XmtpError_StorageError implements XmtpError {
  const factory XmtpError_StorageError(final StorageError field0) =
      _$XmtpError_StorageErrorImpl;

  @override
  StorageError get field0;
  @JsonKey(ignore: true)
  _$$XmtpError_StorageErrorImplCopyWith<_$XmtpError_StorageErrorImpl>
      get copyWith => throw _privateConstructorUsedError;
}

/// @nodoc
abstract class _$$XmtpError_GenericImplCopyWith<$Res> {
  factory _$$XmtpError_GenericImplCopyWith(_$XmtpError_GenericImpl value,
          $Res Function(_$XmtpError_GenericImpl) then) =
      __$$XmtpError_GenericImplCopyWithImpl<$Res>;
  @useResult
  $Res call({AnyhowError field0});
}

/// @nodoc
class __$$XmtpError_GenericImplCopyWithImpl<$Res>
    extends _$XmtpErrorCopyWithImpl<$Res, _$XmtpError_GenericImpl>
    implements _$$XmtpError_GenericImplCopyWith<$Res> {
  __$$XmtpError_GenericImplCopyWithImpl(_$XmtpError_GenericImpl _value,
      $Res Function(_$XmtpError_GenericImpl) _then)
      : super(_value, _then);

  @pragma('vm:prefer-inline')
  @override
  $Res call({
    Object? field0 = null,
  }) {
    return _then(_$XmtpError_GenericImpl(
      null == field0
          ? _value.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as AnyhowError,
    ));
  }
}

/// @nodoc

class _$XmtpError_GenericImpl implements XmtpError_Generic {
  const _$XmtpError_GenericImpl(this.field0);

  @override
  final AnyhowError field0;

  @override
  String toString() {
    return 'XmtpError.generic(field0: $field0)';
  }

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is _$XmtpError_GenericImpl &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @JsonKey(ignore: true)
  @override
  @pragma('vm:prefer-inline')
  _$$XmtpError_GenericImplCopyWith<_$XmtpError_GenericImpl> get copyWith =>
      __$$XmtpError_GenericImplCopyWithImpl<_$XmtpError_GenericImpl>(
          this, _$identity);

  @override
  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function(ApiError field0) apiError,
    required TResult Function(ClientBuilderError field0) clientBuilderError,
    required TResult Function(XmtpMlsClientClientError field0) clientError,
    required TResult Function(StorageError field0) storageError,
    required TResult Function(AnyhowError field0) generic,
  }) {
    return generic(field0);
  }

  @override
  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function(ApiError field0)? apiError,
    TResult? Function(ClientBuilderError field0)? clientBuilderError,
    TResult? Function(XmtpMlsClientClientError field0)? clientError,
    TResult? Function(StorageError field0)? storageError,
    TResult? Function(AnyhowError field0)? generic,
  }) {
    return generic?.call(field0);
  }

  @override
  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function(ApiError field0)? apiError,
    TResult Function(ClientBuilderError field0)? clientBuilderError,
    TResult Function(XmtpMlsClientClientError field0)? clientError,
    TResult Function(StorageError field0)? storageError,
    TResult Function(AnyhowError field0)? generic,
    required TResult orElse(),
  }) {
    if (generic != null) {
      return generic(field0);
    }
    return orElse();
  }

  @override
  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(XmtpError_ApiError value) apiError,
    required TResult Function(XmtpError_ClientBuilderError value)
        clientBuilderError,
    required TResult Function(XmtpError_ClientError value) clientError,
    required TResult Function(XmtpError_StorageError value) storageError,
    required TResult Function(XmtpError_Generic value) generic,
  }) {
    return generic(this);
  }

  @override
  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(XmtpError_ApiError value)? apiError,
    TResult? Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult? Function(XmtpError_ClientError value)? clientError,
    TResult? Function(XmtpError_StorageError value)? storageError,
    TResult? Function(XmtpError_Generic value)? generic,
  }) {
    return generic?.call(this);
  }

  @override
  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(XmtpError_ApiError value)? apiError,
    TResult Function(XmtpError_ClientBuilderError value)? clientBuilderError,
    TResult Function(XmtpError_ClientError value)? clientError,
    TResult Function(XmtpError_StorageError value)? storageError,
    TResult Function(XmtpError_Generic value)? generic,
    required TResult orElse(),
  }) {
    if (generic != null) {
      return generic(this);
    }
    return orElse();
  }
}

abstract class XmtpError_Generic implements XmtpError {
  const factory XmtpError_Generic(final AnyhowError field0) =
      _$XmtpError_GenericImpl;

  @override
  AnyhowError get field0;
  @JsonKey(ignore: true)
  _$$XmtpError_GenericImplCopyWith<_$XmtpError_GenericImpl> get copyWith =>
      throw _privateConstructorUsedError;
}
