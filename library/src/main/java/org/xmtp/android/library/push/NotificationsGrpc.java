package org.xmtp.android.library.push;

import static io.grpc.MethodDescriptor.generateFullMethodName;

/**
 *
 */
@javax.annotation.Generated(
        value = "by gRPC proto compiler (version 1.45.1)",
        comments = "Source: service.proto")
@io.grpc.stub.annotations.GrpcGenerated
public final class NotificationsGrpc {

    private NotificationsGrpc() {
    }

    public static final String SERVICE_NAME = "notifications.v1.Notifications";

    // Static method descriptors that strictly reflect the proto.
    private static volatile io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.RegisterInstallationRequest,
            org.xmtp.android.library.push.Service.RegisterInstallationResponse> getRegisterInstallationMethod;

    @io.grpc.stub.annotations.RpcMethod(
            fullMethodName = SERVICE_NAME + '/' + "RegisterInstallation",
            requestType = org.xmtp.android.library.push.Service.RegisterInstallationRequest.class,
            responseType = org.xmtp.android.library.push.Service.RegisterInstallationResponse.class,
            methodType = io.grpc.MethodDescriptor.MethodType.UNARY)
    public static io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.RegisterInstallationRequest,
            org.xmtp.android.library.push.Service.RegisterInstallationResponse> getRegisterInstallationMethod() {
        io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.RegisterInstallationRequest, org.xmtp.android.library.push.Service.RegisterInstallationResponse> getRegisterInstallationMethod;
        if ((getRegisterInstallationMethod = NotificationsGrpc.getRegisterInstallationMethod) == null) {
            synchronized (NotificationsGrpc.class) {
                if ((getRegisterInstallationMethod = NotificationsGrpc.getRegisterInstallationMethod) == null) {
                    NotificationsGrpc.getRegisterInstallationMethod = getRegisterInstallationMethod =
                            io.grpc.MethodDescriptor.<org.xmtp.android.library.push.Service.RegisterInstallationRequest, org.xmtp.android.library.push.Service.RegisterInstallationResponse>newBuilder()
                                    .setType(io.grpc.MethodDescriptor.MethodType.UNARY)
                                    .setFullMethodName(generateFullMethodName(SERVICE_NAME, "RegisterInstallation"))
                                    .setSampledToLocalTracing(true)
                                    .setRequestMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            org.xmtp.android.library.push.Service.RegisterInstallationRequest.getDefaultInstance()))
                                    .setResponseMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            org.xmtp.android.library.push.Service.RegisterInstallationResponse.getDefaultInstance()))
                                    .build();
                }
            }
        }
        return getRegisterInstallationMethod;
    }

    private static volatile io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.DeleteInstallationRequest,
            com.google.protobuf.Empty> getDeleteInstallationMethod;

    @io.grpc.stub.annotations.RpcMethod(
            fullMethodName = SERVICE_NAME + '/' + "DeleteInstallation",
            requestType = org.xmtp.android.library.push.Service.DeleteInstallationRequest.class,
            responseType = com.google.protobuf.Empty.class,
            methodType = io.grpc.MethodDescriptor.MethodType.UNARY)
    public static io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.DeleteInstallationRequest,
            com.google.protobuf.Empty> getDeleteInstallationMethod() {
        io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.DeleteInstallationRequest, com.google.protobuf.Empty> getDeleteInstallationMethod;
        if ((getDeleteInstallationMethod = NotificationsGrpc.getDeleteInstallationMethod) == null) {
            synchronized (NotificationsGrpc.class) {
                if ((getDeleteInstallationMethod = NotificationsGrpc.getDeleteInstallationMethod) == null) {
                    NotificationsGrpc.getDeleteInstallationMethod = getDeleteInstallationMethod =
                            io.grpc.MethodDescriptor.<org.xmtp.android.library.push.Service.DeleteInstallationRequest, com.google.protobuf.Empty>newBuilder()
                                    .setType(io.grpc.MethodDescriptor.MethodType.UNARY)
                                    .setFullMethodName(generateFullMethodName(SERVICE_NAME, "DeleteInstallation"))
                                    .setSampledToLocalTracing(true)
                                    .setRequestMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            org.xmtp.android.library.push.Service.DeleteInstallationRequest.getDefaultInstance()))
                                    .setResponseMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            com.google.protobuf.Empty.getDefaultInstance()))
                                    .build();
                }
            }
        }
        return getDeleteInstallationMethod;
    }

    private static volatile io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.SubscribeRequest,
            com.google.protobuf.Empty> getSubscribeMethod;

    @io.grpc.stub.annotations.RpcMethod(
            fullMethodName = SERVICE_NAME + '/' + "Subscribe",
            requestType = org.xmtp.android.library.push.Service.SubscribeRequest.class,
            responseType = com.google.protobuf.Empty.class,
            methodType = io.grpc.MethodDescriptor.MethodType.UNARY)
    public static io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.SubscribeRequest,
            com.google.protobuf.Empty> getSubscribeMethod() {
        io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.SubscribeRequest, com.google.protobuf.Empty> getSubscribeMethod;
        if ((getSubscribeMethod = NotificationsGrpc.getSubscribeMethod) == null) {
            synchronized (NotificationsGrpc.class) {
                if ((getSubscribeMethod = NotificationsGrpc.getSubscribeMethod) == null) {
                    NotificationsGrpc.getSubscribeMethod = getSubscribeMethod =
                            io.grpc.MethodDescriptor.<org.xmtp.android.library.push.Service.SubscribeRequest, com.google.protobuf.Empty>newBuilder()
                                    .setType(io.grpc.MethodDescriptor.MethodType.UNARY)
                                    .setFullMethodName(generateFullMethodName(SERVICE_NAME, "Subscribe"))
                                    .setSampledToLocalTracing(true)
                                    .setRequestMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            org.xmtp.android.library.push.Service.SubscribeRequest.getDefaultInstance()))
                                    .setResponseMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            com.google.protobuf.Empty.getDefaultInstance()))
                                    .build();
                }
            }
        }
        return getSubscribeMethod;
    }

    private static volatile io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.UnsubscribeRequest,
            com.google.protobuf.Empty> getUnsubscribeMethod;

    @io.grpc.stub.annotations.RpcMethod(
            fullMethodName = SERVICE_NAME + '/' + "Unsubscribe",
            requestType = org.xmtp.android.library.push.Service.UnsubscribeRequest.class,
            responseType = com.google.protobuf.Empty.class,
            methodType = io.grpc.MethodDescriptor.MethodType.UNARY)
    public static io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.UnsubscribeRequest,
            com.google.protobuf.Empty> getUnsubscribeMethod() {
        io.grpc.MethodDescriptor<org.xmtp.android.library.push.Service.UnsubscribeRequest, com.google.protobuf.Empty> getUnsubscribeMethod;
        if ((getUnsubscribeMethod = NotificationsGrpc.getUnsubscribeMethod) == null) {
            synchronized (NotificationsGrpc.class) {
                if ((getUnsubscribeMethod = NotificationsGrpc.getUnsubscribeMethod) == null) {
                    NotificationsGrpc.getUnsubscribeMethod = getUnsubscribeMethod =
                            io.grpc.MethodDescriptor.<org.xmtp.android.library.push.Service.UnsubscribeRequest, com.google.protobuf.Empty>newBuilder()
                                    .setType(io.grpc.MethodDescriptor.MethodType.UNARY)
                                    .setFullMethodName(generateFullMethodName(SERVICE_NAME, "Unsubscribe"))
                                    .setSampledToLocalTracing(true)
                                    .setRequestMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            org.xmtp.android.library.push.Service.UnsubscribeRequest.getDefaultInstance()))
                                    .setResponseMarshaller(io.grpc.protobuf.lite.ProtoLiteUtils.marshaller(
                                            com.google.protobuf.Empty.getDefaultInstance()))
                                    .build();
                }
            }
        }
        return getUnsubscribeMethod;
    }

    /**
     * Creates a new async stub that supports all call types for the service
     */
    public static NotificationsStub newStub(io.grpc.Channel channel) {
        io.grpc.stub.AbstractStub.StubFactory<NotificationsStub> factory =
                new io.grpc.stub.AbstractStub.StubFactory<NotificationsStub>() {
                    @java.lang.Override
                    public NotificationsStub newStub(io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
                        return new NotificationsStub(channel, callOptions);
                    }
                };
        return NotificationsStub.newStub(factory, channel);
    }

    /**
     * Creates a new blocking-style stub that supports unary and streaming output calls on the service
     */
    public static NotificationsBlockingStub newBlockingStub(
            io.grpc.Channel channel) {
        io.grpc.stub.AbstractStub.StubFactory<NotificationsBlockingStub> factory =
                new io.grpc.stub.AbstractStub.StubFactory<NotificationsBlockingStub>() {
                    @java.lang.Override
                    public NotificationsBlockingStub newStub(io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
                        return new NotificationsBlockingStub(channel, callOptions);
                    }
                };
        return NotificationsBlockingStub.newStub(factory, channel);
    }

    /**
     * Creates a new ListenableFuture-style stub that supports unary calls on the service
     */
    public static NotificationsFutureStub newFutureStub(
            io.grpc.Channel channel) {
        io.grpc.stub.AbstractStub.StubFactory<NotificationsFutureStub> factory =
                new io.grpc.stub.AbstractStub.StubFactory<NotificationsFutureStub>() {
                    @java.lang.Override
                    public NotificationsFutureStub newStub(io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
                        return new NotificationsFutureStub(channel, callOptions);
                    }
                };
        return NotificationsFutureStub.newStub(factory, channel);
    }

    /**
     *
     */
    public static abstract class NotificationsImplBase implements io.grpc.BindableService {

        /**
         *
         */
        public void registerInstallation(org.xmtp.android.library.push.Service.RegisterInstallationRequest request,
                                         io.grpc.stub.StreamObserver<org.xmtp.android.library.push.Service.RegisterInstallationResponse> responseObserver) {
            io.grpc.stub.ServerCalls.asyncUnimplementedUnaryCall(getRegisterInstallationMethod(), responseObserver);
        }

        /**
         *
         */
        public void deleteInstallation(org.xmtp.android.library.push.Service.DeleteInstallationRequest request,
                                       io.grpc.stub.StreamObserver<com.google.protobuf.Empty> responseObserver) {
            io.grpc.stub.ServerCalls.asyncUnimplementedUnaryCall(getDeleteInstallationMethod(), responseObserver);
        }

        /**
         *
         */
        public void subscribe(org.xmtp.android.library.push.Service.SubscribeRequest request,
                              io.grpc.stub.StreamObserver<com.google.protobuf.Empty> responseObserver) {
            io.grpc.stub.ServerCalls.asyncUnimplementedUnaryCall(getSubscribeMethod(), responseObserver);
        }

        /**
         *
         */
        public void unsubscribe(org.xmtp.android.library.push.Service.UnsubscribeRequest request,
                                io.grpc.stub.StreamObserver<com.google.protobuf.Empty> responseObserver) {
            io.grpc.stub.ServerCalls.asyncUnimplementedUnaryCall(getUnsubscribeMethod(), responseObserver);
        }

        @java.lang.Override
        public final io.grpc.ServerServiceDefinition bindService() {
            return io.grpc.ServerServiceDefinition.builder(getServiceDescriptor())
                    .addMethod(
                            getRegisterInstallationMethod(),
                            io.grpc.stub.ServerCalls.asyncUnaryCall(
                                    new MethodHandlers<
                                            org.xmtp.android.library.push.Service.RegisterInstallationRequest,
                                            org.xmtp.android.library.push.Service.RegisterInstallationResponse>(
                                            this, METHODID_REGISTER_INSTALLATION)))
                    .addMethod(
                            getDeleteInstallationMethod(),
                            io.grpc.stub.ServerCalls.asyncUnaryCall(
                                    new MethodHandlers<
                                            org.xmtp.android.library.push.Service.DeleteInstallationRequest,
                                            com.google.protobuf.Empty>(
                                            this, METHODID_DELETE_INSTALLATION)))
                    .addMethod(
                            getSubscribeMethod(),
                            io.grpc.stub.ServerCalls.asyncUnaryCall(
                                    new MethodHandlers<
                                            org.xmtp.android.library.push.Service.SubscribeRequest,
                                            com.google.protobuf.Empty>(
                                            this, METHODID_SUBSCRIBE)))
                    .addMethod(
                            getUnsubscribeMethod(),
                            io.grpc.stub.ServerCalls.asyncUnaryCall(
                                    new MethodHandlers<
                                            org.xmtp.android.library.push.Service.UnsubscribeRequest,
                                            com.google.protobuf.Empty>(
                                            this, METHODID_UNSUBSCRIBE)))
                    .build();
        }
    }

    /**
     *
     */
    public static final class NotificationsStub extends io.grpc.stub.AbstractAsyncStub<NotificationsStub> {
        private NotificationsStub(
                io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
            super(channel, callOptions);
        }

        @java.lang.Override
        protected NotificationsStub build(
                io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
            return new NotificationsStub(channel, callOptions);
        }

        /**
         *
         */
        public void registerInstallation(org.xmtp.android.library.push.Service.RegisterInstallationRequest request,
                                         io.grpc.stub.StreamObserver<org.xmtp.android.library.push.Service.RegisterInstallationResponse> responseObserver) {
            io.grpc.stub.ClientCalls.asyncUnaryCall(
                    getChannel().newCall(getRegisterInstallationMethod(), getCallOptions()), request, responseObserver);
        }

        /**
         *
         */
        public void deleteInstallation(org.xmtp.android.library.push.Service.DeleteInstallationRequest request,
                                       io.grpc.stub.StreamObserver<com.google.protobuf.Empty> responseObserver) {
            io.grpc.stub.ClientCalls.asyncUnaryCall(
                    getChannel().newCall(getDeleteInstallationMethod(), getCallOptions()), request, responseObserver);
        }

        /**
         *
         */
        public void subscribe(org.xmtp.android.library.push.Service.SubscribeRequest request,
                              io.grpc.stub.StreamObserver<com.google.protobuf.Empty> responseObserver) {
            io.grpc.stub.ClientCalls.asyncUnaryCall(
                    getChannel().newCall(getSubscribeMethod(), getCallOptions()), request, responseObserver);
        }

        /**
         *
         */
        public void unsubscribe(org.xmtp.android.library.push.Service.UnsubscribeRequest request,
                                io.grpc.stub.StreamObserver<com.google.protobuf.Empty> responseObserver) {
            io.grpc.stub.ClientCalls.asyncUnaryCall(
                    getChannel().newCall(getUnsubscribeMethod(), getCallOptions()), request, responseObserver);
        }
    }

    /**
     *
     */
    public static final class NotificationsBlockingStub extends io.grpc.stub.AbstractBlockingStub<NotificationsBlockingStub> {
        private NotificationsBlockingStub(
                io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
            super(channel, callOptions);
        }

        @java.lang.Override
        protected NotificationsBlockingStub build(
                io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
            return new NotificationsBlockingStub(channel, callOptions);
        }

        /**
         *
         */
        public org.xmtp.android.library.push.Service.RegisterInstallationResponse registerInstallation(org.xmtp.android.library.push.Service.RegisterInstallationRequest request) {
            return io.grpc.stub.ClientCalls.blockingUnaryCall(
                    getChannel(), getRegisterInstallationMethod(), getCallOptions(), request);
        }

        /**
         *
         */
        public com.google.protobuf.Empty deleteInstallation(org.xmtp.android.library.push.Service.DeleteInstallationRequest request) {
            return io.grpc.stub.ClientCalls.blockingUnaryCall(
                    getChannel(), getDeleteInstallationMethod(), getCallOptions(), request);
        }

        /**
         *
         */
        public com.google.protobuf.Empty subscribe(org.xmtp.android.library.push.Service.SubscribeRequest request) {
            return io.grpc.stub.ClientCalls.blockingUnaryCall(
                    getChannel(), getSubscribeMethod(), getCallOptions(), request);
        }

        /**
         *
         */
        public com.google.protobuf.Empty unsubscribe(org.xmtp.android.library.push.Service.UnsubscribeRequest request) {
            return io.grpc.stub.ClientCalls.blockingUnaryCall(
                    getChannel(), getUnsubscribeMethod(), getCallOptions(), request);
        }
    }

    /**
     *
     */
    public static final class NotificationsFutureStub extends io.grpc.stub.AbstractFutureStub<NotificationsFutureStub> {
        private NotificationsFutureStub(
                io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
            super(channel, callOptions);
        }

        @java.lang.Override
        protected NotificationsFutureStub build(
                io.grpc.Channel channel, io.grpc.CallOptions callOptions) {
            return new NotificationsFutureStub(channel, callOptions);
        }

        /**
         *
         */
        public com.google.common.util.concurrent.ListenableFuture<org.xmtp.android.library.push.Service.RegisterInstallationResponse> registerInstallation(
                org.xmtp.android.library.push.Service.RegisterInstallationRequest request) {
            return io.grpc.stub.ClientCalls.futureUnaryCall(
                    getChannel().newCall(getRegisterInstallationMethod(), getCallOptions()), request);
        }

        /**
         *
         */
        public com.google.common.util.concurrent.ListenableFuture<com.google.protobuf.Empty> deleteInstallation(
                org.xmtp.android.library.push.Service.DeleteInstallationRequest request) {
            return io.grpc.stub.ClientCalls.futureUnaryCall(
                    getChannel().newCall(getDeleteInstallationMethod(), getCallOptions()), request);
        }

        /**
         *
         */
        public com.google.common.util.concurrent.ListenableFuture<com.google.protobuf.Empty> subscribe(
                org.xmtp.android.library.push.Service.SubscribeRequest request) {
            return io.grpc.stub.ClientCalls.futureUnaryCall(
                    getChannel().newCall(getSubscribeMethod(), getCallOptions()), request);
        }

        /**
         *
         */
        public com.google.common.util.concurrent.ListenableFuture<com.google.protobuf.Empty> unsubscribe(
                org.xmtp.android.library.push.Service.UnsubscribeRequest request) {
            return io.grpc.stub.ClientCalls.futureUnaryCall(
                    getChannel().newCall(getUnsubscribeMethod(), getCallOptions()), request);
        }
    }

    private static final int METHODID_REGISTER_INSTALLATION = 0;
    private static final int METHODID_DELETE_INSTALLATION = 1;
    private static final int METHODID_SUBSCRIBE = 2;
    private static final int METHODID_UNSUBSCRIBE = 3;

    private static final class MethodHandlers<Req, Resp> implements
            io.grpc.stub.ServerCalls.UnaryMethod<Req, Resp>,
            io.grpc.stub.ServerCalls.ServerStreamingMethod<Req, Resp>,
            io.grpc.stub.ServerCalls.ClientStreamingMethod<Req, Resp>,
            io.grpc.stub.ServerCalls.BidiStreamingMethod<Req, Resp> {
        private final NotificationsImplBase serviceImpl;
        private final int methodId;

        MethodHandlers(NotificationsImplBase serviceImpl, int methodId) {
            this.serviceImpl = serviceImpl;
            this.methodId = methodId;
        }

        @java.lang.Override
        @java.lang.SuppressWarnings("unchecked")
        public void invoke(Req request, io.grpc.stub.StreamObserver<Resp> responseObserver) {
            switch (methodId) {
                case METHODID_REGISTER_INSTALLATION:
                    serviceImpl.registerInstallation((org.xmtp.android.library.push.Service.RegisterInstallationRequest) request,
                            (io.grpc.stub.StreamObserver<org.xmtp.android.library.push.Service.RegisterInstallationResponse>) responseObserver);
                    break;
                case METHODID_DELETE_INSTALLATION:
                    serviceImpl.deleteInstallation((org.xmtp.android.library.push.Service.DeleteInstallationRequest) request,
                            (io.grpc.stub.StreamObserver<com.google.protobuf.Empty>) responseObserver);
                    break;
                case METHODID_SUBSCRIBE:
                    serviceImpl.subscribe((org.xmtp.android.library.push.Service.SubscribeRequest) request,
                            (io.grpc.stub.StreamObserver<com.google.protobuf.Empty>) responseObserver);
                    break;
                case METHODID_UNSUBSCRIBE:
                    serviceImpl.unsubscribe((org.xmtp.android.library.push.Service.UnsubscribeRequest) request,
                            (io.grpc.stub.StreamObserver<com.google.protobuf.Empty>) responseObserver);
                    break;
                default:
                    throw new AssertionError();
            }
        }

        @java.lang.Override
        @java.lang.SuppressWarnings("unchecked")
        public io.grpc.stub.StreamObserver<Req> invoke(
                io.grpc.stub.StreamObserver<Resp> responseObserver) {
            switch (methodId) {
                default:
                    throw new AssertionError();
            }
        }
    }

    private static volatile io.grpc.ServiceDescriptor serviceDescriptor;

    public static io.grpc.ServiceDescriptor getServiceDescriptor() {
        io.grpc.ServiceDescriptor result = serviceDescriptor;
        if (result == null) {
            synchronized (NotificationsGrpc.class) {
                result = serviceDescriptor;
                if (result == null) {
                    serviceDescriptor = result = io.grpc.ServiceDescriptor.newBuilder(SERVICE_NAME)
                            .addMethod(getRegisterInstallationMethod())
                            .addMethod(getDeleteInstallationMethod())
                            .addMethod(getSubscribeMethod())
                            .addMethod(getUnsubscribeMethod())
                            .build();
                }
            }
        }
        return result;
    }
}
