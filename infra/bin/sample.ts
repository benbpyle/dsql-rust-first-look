#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import { SampleStack } from '../lib/sample-stack';

const app = new cdk.App();
new SampleStack(app, 'SampleStack', {
  env: { region: 'us-east-1' },
});
