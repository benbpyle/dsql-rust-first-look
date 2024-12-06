import * as cdk from 'aws-cdk-lib';
import { Effect, PolicyStatement } from 'aws-cdk-lib/aws-iam';
import { Architecture, FunctionUrlAuthType, LayerVersion } from 'aws-cdk-lib/aws-lambda';
import { RustFunction } from 'cargo-lambda-cdk';
import { Construct } from 'constructs';
// import * as sqs from 'aws-cdk-lib/aws-sqs';

export class SampleStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    const layer = LayerVersion.fromLayerVersionArn(
      this,
      'DatadogExtension',
      'arn:aws:lambda:us-east-1:464622532012:layer:Datadog-Extension-ARM:67'
    )

    const insert = new RustFunction(this, 'InsertFunction', {
      architecture: Architecture.ARM_64,
      functionName: "dsql-insert",
      manifestPath: 'lambdas/insert',
      memorySize: 256,
      environment: {
        CLUSTER_ENDPOINT: "qqabtvdilkxepq6xxfreja7vvm.dsql.us-east-1.on.aws",
        DD_API_KEY: process.env.DD_API_KEY!,
        DD_SERVICE: 'dsql-insert',
        DD_SITE: process.env.DD_SITE!,
        RUST_LOG: 'info',
      },
      layers: [layer]
    })

    const select = new RustFunction(this, 'SelectFunction', {
      architecture: Architecture.ARM_64,
      functionName: "dsql-select",
      manifestPath: 'lambdas/select',
      memorySize: 256,
      environment: {
        CLUSTER_ENDPOINT: "qqabtvdilkxepq6xxfreja7vvm.dsql.us-east-1.on.aws",
        DD_API_KEY: process.env.DD_API_KEY!,
        DD_SERVICE: 'dsql-select',
        DD_SITE: process.env.DD_SITE!,
        RUST_LOG: 'info',
      },
      layers: [layer]
    })

    select.addToRolePolicy(new PolicyStatement({
      effect: Effect.ALLOW,
      actions: ["dsql:*"],
      resources: ["*"]
    }))

    select.addFunctionUrl({
      authType: FunctionUrlAuthType.NONE,
      cors: {
        allowedOrigins: ["*"]
      }
    })

    insert.addToRolePolicy(new PolicyStatement({
      effect: Effect.ALLOW,
      actions: ["dsql:*"],
      resources: ["*"]
    }))

    insert.addFunctionUrl({
      authType: FunctionUrlAuthType.NONE,
      cors: {
        allowedOrigins: ["*"]
      }
    })
  }
}
