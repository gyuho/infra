package nodes

import (
	"testing"

	core_v1 "k8s.io/api/core/v1"
)

func TestIsReady(t *testing.T) {
	readyNode := &core_v1.Node{
		Status: core_v1.NodeStatus{
			Conditions: []core_v1.NodeCondition{
				{
					Type:   core_v1.NodeReady,
					Status: core_v1.ConditionTrue,
				},
			},
		},
	}

	notReadyNode := &core_v1.Node{
		Status: core_v1.NodeStatus{
			Conditions: []core_v1.NodeCondition{
				{
					Type:   core_v1.NodeReady,
					Status: core_v1.ConditionFalse,
				},
			},
		},
	}

	// Test for a ready node
	if !IsReady(readyNode) {
		t.Errorf("Expected IsReady to return true for a ready node, but got false")
	}

	// Test for a not ready node
	if IsReady(notReadyNode) {
		t.Errorf("Expected IsReady to return false for a not ready node, but got true")
	}
}

func TestMatchConditionType(t *testing.T) {
	status := core_v1.NodeStatus{
		Conditions: []core_v1.NodeCondition{
			{
				Type:   core_v1.NodeReady,
				Status: core_v1.ConditionTrue,
			},
			{
				Type:   core_v1.NodeDiskPressure,
				Status: core_v1.ConditionFalse,
			},
		},
	}

	// Test for a matching status
	if !matchConditionType(status, core_v1.NodeReady) {
		t.Errorf("Expected matchStatus to return true for a matching status, but got false")
	}

	// Test for a non-matching status
	if matchConditionType(status, core_v1.NodeDiskPressure) {
		t.Errorf("Expected matchStatus to return false for a non-matching status, but got true")
	}
}
